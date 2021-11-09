use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens, TokenStreamExt};
use syn::Type;

use sqlx_core::column::Column;
use sqlx_core::describe::Describe;

use crate::database::DatabaseExt;

use crate::query::QueryMacroInput;
use std::fmt::{self, Display, Formatter};
use syn::parse::{Parse, ParseStream};
use syn::Token;

pub struct RustColumn {
    pub(super) ident: Ident,
    pub(super) type_: ColumnType,
}

pub(super) enum ColumnType {
    Exact(TokenStream),
    Wildcard,
    OptWildcard,
}

impl ColumnType {
    pub(super) fn is_wildcard(&self) -> bool {
        match self {
            ColumnType::Exact(_) => false,
            _ => true,
        }
    }
}

impl ToTokens for ColumnType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append_all(match &self {
            ColumnType::Exact(type_) => type_.clone().into_iter(),
            ColumnType::Wildcard => quote! { _ }.into_iter(),
            ColumnType::OptWildcard => quote! { Option<_> }.into_iter(),
        })
    }
}

struct DisplayColumn<'a> {
    // zero-based index, converted to 1-based number
    idx: usize,
    name: &'a str,
}

struct ColumnDecl {
    ident: Ident,
    r#override: ColumnOverride,
}

struct ColumnOverride {
    nullability: ColumnNullabilityOverride,
    type_: ColumnTypeOverride,
}

#[derive(PartialEq)]
enum ColumnNullabilityOverride {
    NonNull,
    Nullable,
    None,
}

enum ColumnTypeOverride {
    Exact(Type),
    Wildcard,
    None,
}

impl Display for DisplayColumn<'_> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "column #{} ({:?})", self.idx + 1, self.name)
    }
}

pub fn columns_to_rust<DB: DatabaseExt>(describe: &Describe<DB>) -> crate::Result<Vec<RustColumn>> {
    describe
        .columns()
        .iter()
        .enumerate()
        .map(|(i, column)| -> crate::Result<_> {
            // add raw prefix to all identifiers
            let decl = ColumnDecl::parse(&column.name())
                .map_err(|e| format!("column name {:?} is invalid: {}", column.name(), e))?;

            let ColumnOverride { nullability, type_ } = decl.r#override;

            let nullable = match nullability {
                ColumnNullabilityOverride::NonNull => false,
                ColumnNullabilityOverride::Nullable => true,
                ColumnNullabilityOverride::None => describe.nullable(i).unwrap_or(true),
            };
            let type_ = match (type_, nullable) {
                (ColumnTypeOverride::Exact(type_), false) => {
                    ColumnType::Exact(type_.to_token_stream())
                }
                (ColumnTypeOverride::Exact(type_), true) => {
                    ColumnType::Exact(quote! { Option<#type_> })
                }

                (ColumnTypeOverride::Wildcard, false) => ColumnType::Wildcard,
                (ColumnTypeOverride::Wildcard, true) => ColumnType::OptWildcard,

                (ColumnTypeOverride::None, _) => {
                    let type_ = get_column_type::<DB>(i, column);
                    if !nullable {
                        ColumnType::Exact(type_)
                    } else {
                        ColumnType::Exact(quote! { Option<#type_> })
                    }
                }
            };

            Ok(RustColumn {
                ident: decl.ident,
                type_,
            })
        })
        .collect::<crate::Result<Vec<_>>>()
}

pub fn quote_query_as<DB: DatabaseExt>(
    input: &QueryMacroInput,
    out_ty: &Type,
    bind_args: &Ident,
    columns: &[RustColumn],
) -> TokenStream {
    let instantiations = columns.iter().enumerate().map(
        |(
            i,
            &RustColumn {
                ref ident,
                ref type_,
                ..
            },
        )| {
            match (input.checked, type_) {
                // we guarantee the type is valid so we can skip the runtime check
                (true, ColumnType::Exact(type_)) => quote! {
                    // binding to a `let` avoids confusing errors about
                    // "try expression alternatives have incompatible types"
                    // it doesn't seem to hurt inference in the other branches
                    let #ident = row.try_get_unchecked::<#type_, _>(#i)?;
                },
                // type was overridden to be a wildcard so we fallback to the runtime check
                (true, ColumnType::Wildcard) => quote! ( let #ident = row.try_get(#i)?; ),
                (true, ColumnType::OptWildcard) => {
                    quote! ( let #ident = row.try_get::<Option<_>, _>(#i)?; )
                }
                // macro is the `_unchecked!()` variant so this will die in decoding if it's wrong
                (false, _) => quote!( let #ident = row.try_get_unchecked(#i)?; ),
            }
        },
    );

    let ident = columns.iter().map(|col| &col.ident);

    let db_path = DB::db_path();
    let row_path = DB::row_path();
    let sql = &input.src;

    quote! {
        sqlx::query_with::<#db_path, _>(#sql, #bind_args).try_map(|row: #row_path| {
            use sqlx::Row as _;

            #(#instantiations)*

            Ok(#out_ty { #(#ident: #ident),* })
        })
    }
}

fn get_column_type<DB: DatabaseExt>(i: usize, column: &DB::Column) -> TokenStream {
    let type_info = &*column.type_info();

    <DB as DatabaseExt>::return_type_for_id(&type_info).map_or_else(
        || {
            let message =
                if let Some(feature_gate) = <DB as DatabaseExt>::get_feature_gate(&type_info) {
                    format!(
                        "optional feature `{feat}` required for type {ty} of {col}",
                        ty = &type_info,
                        feat = feature_gate,
                        col = DisplayColumn {
                            idx: i,
                            name: &*column.name()
                        }
                    )
                } else {
                    format!(
                        "unsupported type {ty} of {col}",
                        ty = type_info,
                        col = DisplayColumn {
                            idx: i,
                            name: &*column.name()
                        }
                    )
                };
            syn::Error::new(Span::call_site(), message).to_compile_error()
        },
        |t| t.parse().unwrap(),
    )
}

impl ColumnDecl {
    fn parse(col_name: &str) -> crate::Result<Self> {
        // find the end of the identifier because we want to use our own logic to parse it
        // if we tried to feed this into `syn::parse_str()` we might get an un-great error
        // for some kinds of invalid identifiers
        let (ident, remainder) = if let Some(i) = col_name.find(&[':', '!', '?'][..]) {
            let (ident, remainder) = col_name.split_at(i);

            (parse_ident(ident)?, remainder)
        } else {
            (parse_ident(col_name)?, "")
        };

        Ok(ColumnDecl {
            ident,
            r#override: if !remainder.is_empty() {
                syn::parse_str(remainder)?
            } else {
                ColumnOverride {
                    nullability: ColumnNullabilityOverride::None,
                    type_: ColumnTypeOverride::None,
                }
            },
        })
    }
}

impl Parse for ColumnOverride {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let lookahead = input.lookahead1();

        let nullability = if lookahead.peek(Token![!]) {
            input.parse::<Token![!]>()?;

            ColumnNullabilityOverride::NonNull
        } else if lookahead.peek(Token![?]) {
            input.parse::<Token![?]>()?;

            ColumnNullabilityOverride::Nullable
        } else {
            ColumnNullabilityOverride::None
        };

        let type_ = if input.lookahead1().peek(Token![:]) {
            input.parse::<Token![:]>()?;

            let ty = Type::parse(input)?;

            if let Type::Infer(_) = ty {
                ColumnTypeOverride::Wildcard
            } else {
                ColumnTypeOverride::Exact(ty)
            }
        } else {
            ColumnTypeOverride::None
        };

        Ok(Self { nullability, type_ })
    }
}

fn parse_ident(name: &str) -> crate::Result<Ident> {
    // workaround for the following issue (it's semi-fixed but still spits out extra diagnostics)
    // https://github.com/dtolnay/syn/issues/749#issuecomment-575451318

    let is_valid_ident = name.chars().all(|c| c.is_alphanumeric() || c == '_');

    if is_valid_ident {
        let ident = String::from("r#") + name;
        if let Ok(ident) = syn::parse_str(&ident) {
            return Ok(ident);
        }
    }

    Err(format!("{:?} is not a valid Rust identifier", name).into())
}

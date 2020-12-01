mod create;
mod delete;
mod edit;
mod get;
mod invites;

pub use create::*;
pub use delete::*;
pub use edit::*;
pub use get::*;
pub use invites::*;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct SquadSelectionInput {
    squad_id: i64
}
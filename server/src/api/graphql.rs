mod stats;

use crate::api;
use squadov_common::stats::StatPermission;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use juniper::http::GraphQLRequest;
use juniper::http::graphiql::graphiql_source;
use std::sync::Arc;
use juniper::FieldResult;

pub struct GraphqlContext {
    app: Arc<api::ApiApplication>,
    session: Option<api::auth::SquadOVSession>
}

impl GraphqlContext {
    pub fn has_access_to_stat(&self, stats: &[StatPermission]) -> FieldResult<bool> {
        Ok(stats.iter().all(|x| {
            if self.session.is_none() {
                log::warn!("Denying GraphQL due to no session.");
                return false;
            }

            let session = self.session.as_ref().unwrap();
            if session.share_token.is_none() {
                return true;
            }

            let share_token = session.share_token.as_ref().unwrap();
            if share_token.graphql_stats.is_none() {
                log::warn!("Denying GraphQL due to no stats on share token.");
                return false;
            }

            let ok_stats = share_token.graphql_stats.as_ref().unwrap();
            ok_stats.contains(x)
        }))
    }
}

impl juniper::Context for GraphqlContext {}

pub struct GraphqlRootQuery {
}

#[juniper::graphql_object(
    Context = GraphqlContext,
)]
impl GraphqlRootQuery {
    async fn stats(context: &GraphqlContext, user_id: String) -> FieldResult<stats::GraphqlAllStats> {
        let user_id = user_id.parse::<i64>()?;
        if !context.app.config.server.graphql_debug {
            // Need to verify that the current user has access to this particular user's stats.
            let user = match context.app.users.get_stored_user_from_id(user_id, &*context.app.pool).await? {
                Some(x) => x,
                None => return Err(juniper::FieldError::new("Invalid user.", juniper::Value::Null))
            };

            if user.id != context.session.as_ref().unwrap().user.id {
                return Err(juniper::FieldError::new("No stat access to user.", juniper::Value::Null));
            }
        }

        Ok(stats::GraphqlAllStats{
            user_id, 
        })
    }
}

pub type GraphqlSchema = juniper::RootNode<'static, GraphqlRootQuery, juniper::EmptyMutation<GraphqlContext>, juniper::EmptySubscription<GraphqlContext>>;
pub fn create_schema() -> GraphqlSchema {
    GraphqlSchema::new(GraphqlRootQuery{}, juniper::EmptyMutation::new(), juniper::EmptySubscription::new())
}

pub async fn graphql_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<GraphQLRequest>, req: HttpRequest) -> Result<HttpResponse, squadov_common::SquadOvError> {
    let context = Arc::new(GraphqlContext{
        app: app.get_ref().clone(),
        session: {
            let extensions = req.extensions();
            match extensions.get::<api::auth::SquadOVSession>() {
                Some(s) => Some(s.clone()),
                None => return Err(squadov_common::SquadOvError::Unauthorized),
            }
        },
    });
    let resp = data.execute(&app.schema, &context).await;
    Ok(HttpResponse::Ok().json(&resp))
}

pub async fn graphiql_handler() -> HttpResponse {
    let html = graphiql_source("http://127.0.0.1:8080/graphql", None);
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}
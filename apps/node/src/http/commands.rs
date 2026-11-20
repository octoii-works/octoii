//! The HTTP carrier for the assembled product methods.

use axum::extract::{Extension, Json, State, rejection::JsonRejection};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use octoii::connection::{Access, call, refusal};
use octoii::protocol::{Command, CommandReply, Error};
use octoii_tunnel::Remote;

use super::{Node, session};

pub(super) async fn called(
    State(node): State<Node>,
    headers: HeaderMap,
    remote: Option<Extension<Remote>>,
    command: Result<Json<Command>, JsonRejection>,
) -> Response {
    let granted = match session(&node, &headers, remote.as_deref()) {
        Ok(granted) => granted,
        Err(denied) => {
            let code = match denied.0 {
                StatusCode::UNAUTHORIZED => "unauthorized",
                StatusCode::FORBIDDEN => "forbidden",
                _ => "failed",
            };
            return refused(denied.0, refusal(code, &denied.1));
        }
    };
    let command = match command {
        Ok(Json(command)) => command,
        Err(error) => return refused(error.status(), refusal("bad_call", &error.body_text())),
    };
    let access = Access {
        session: granted.id,
        scopes: granted.scopes,
    };
    // Once admitted, a command finishes even if the client stops waiting for its HTTP response.
    match tokio::spawn(call(node.siphon, access, command)).await {
        Ok(Ok(result)) => reply(StatusCode::OK, CommandReply::Result { result }),
        Ok(Err(error)) => {
            let status = match error.code.as_str() {
                "bad_params" => StatusCode::BAD_REQUEST,
                "no_such_method" | "no_such_thread" => StatusCode::NOT_FOUND,
                "forbidden" => StatusCode::FORBIDDEN,
                _ => StatusCode::UNPROCESSABLE_ENTITY,
            };
            refused(status, error)
        }
        Err(_) => refused(StatusCode::INTERNAL_SERVER_ERROR, refusal("failed", "the command did not finish")),
    }
}

fn refused(status: StatusCode, error: Error) -> Response {
    reply(status, CommandReply::Error { error })
}

fn reply(status: StatusCode, message: CommandReply) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(message)).into_response()
}

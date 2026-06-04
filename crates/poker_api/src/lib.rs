use std::{
    fmt,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use axum::{
    Json, Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{any, get, post},
};
use poker_engine::TableConfig;
use poker_server::{RoomCommand, RoomCommandResult, RoomId, RoomManager, RoomManagerError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ApiState {
    rooms: Arc<RwLock<RoomManager>>,
}

impl ApiState {
    pub fn new(room_manager: RoomManager) -> Self {
        Self {
            rooms: Arc::new(RwLock::new(room_manager)),
        }
    }
}

impl Default for ApiState {
    fn default() -> Self {
        Self::new(RoomManager::new())
    }
}

pub fn app(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/rooms", post(create_room))
        .route("/rooms/{room_id}/commands", post(handle_room_command))
        .route("/rooms/{room_id}/ws", any(room_websocket))
        .with_state(state)
}

pub async fn serve(addr: SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app(ApiState::default())).await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    status: String,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRoomRequest {
    id: RoomId,
    table_config: TableConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    id: RoomId,
}

async fn create_room(
    State(state): State<ApiState>,
    Json(request): Json<CreateRoomRequest>,
) -> Result<(StatusCode, Json<CreateRoomResponse>), ApiError> {
    let mut rooms = state
        .rooms
        .write()
        .map_err(|_| ApiError::StateLockPoisoned)?;
    rooms.create_room(request.id, request.table_config)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateRoomResponse { id: request.id }),
    ))
}

async fn handle_room_command(
    Path(room_id): Path<u64>,
    State(state): State<ApiState>,
    Json(command): Json<RoomCommand>,
) -> Result<Json<RoomCommandResult>, ApiError> {
    let result = dispatch_room_command(&state, RoomId(room_id), command)?;

    Ok(Json(result))
}

async fn room_websocket(
    Path(room_id): Path<u64>,
    State(state): State<ApiState>,
    websocket: WebSocketUpgrade,
) -> Response {
    websocket
        .on_upgrade(move |socket| handle_room_socket(socket, state, RoomId(room_id)))
        .into_response()
}

async fn handle_room_socket(mut socket: WebSocket, state: ApiState, room_id: RoomId) {
    while let Some(message) = socket.recv().await {
        let response = match message {
            Ok(Message::Text(text)) => handle_room_socket_text(&state, room_id, text.as_ref()),
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            Ok(Message::Binary(_)) => WebSocketServerMessage::Error(ApiErrorBody {
                error: "expected text JSON room command".to_string(),
            }),
            Err(error) => WebSocketServerMessage::Error(ApiErrorBody {
                error: format!("websocket receive error: {error}"),
            }),
        };

        let Ok(payload) = serde_json::to_string(&response) else {
            break;
        };

        if socket.send(Message::Text(payload.into())).await.is_err() {
            break;
        }
    }
}

fn handle_room_socket_text(
    state: &ApiState,
    room_id: RoomId,
    text: &str,
) -> WebSocketServerMessage {
    let command = match serde_json::from_str::<RoomCommand>(text) {
        Ok(command) => command,
        Err(error) => {
            return WebSocketServerMessage::Error(ApiErrorBody {
                error: format!("invalid room command JSON: {error}"),
            });
        }
    };

    match dispatch_room_command(state, room_id, command) {
        Ok(result) => WebSocketServerMessage::CommandResult(result),
        Err(error) => WebSocketServerMessage::Error(ApiErrorBody {
            error: error.to_string(),
        }),
    }
}

fn dispatch_room_command(
    state: &ApiState,
    room_id: RoomId,
    command: RoomCommand,
) -> Result<RoomCommandResult, ApiError> {
    let rooms = state
        .rooms
        .read()
        .map_err(|_| ApiError::StateLockPoisoned)?;

    Ok(rooms.handle_room_command(room_id, command)?)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSocketServerMessage {
    CommandResult(RoomCommandResult),
    Error(ApiErrorBody),
}

#[derive(Debug)]
pub enum ApiError {
    StateLockPoisoned,
    RoomManager(RoomManagerError),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateLockPoisoned => write!(f, "api state lock was poisoned"),
            Self::RoomManager(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<RoomManagerError> for ApiError {
    fn from(error: RoomManagerError) -> Self {
        Self::RoomManager(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::StateLockPoisoned => StatusCode::INTERNAL_SERVER_ERROR,
            Self::RoomManager(RoomManagerError::RoomAlreadyExists { .. }) => StatusCode::CONFLICT,
            Self::RoomManager(RoomManagerError::RoomNotFound { .. }) => StatusCode::NOT_FOUND,
            Self::RoomManager(RoomManagerError::RoomLockPoisoned { .. }) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::RoomManager(RoomManagerError::Room(_)) => StatusCode::BAD_REQUEST,
        };

        (
            status,
            Json(ApiErrorBody {
                error: self.to_string(),
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    error: String,
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use poker_engine::{PlayerAction, PlayerId, SeatIndex, TableConfig};
    use tower::ServiceExt;

    use super::*;

    fn table_config() -> TableConfig {
        TableConfig::new(6, 5, 10, 100, 1_000).expect("table config should be valid")
    }

    async fn response_json<T>(response: Response) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");

        serde_json::from_slice(&body).expect("response body should be valid json")
    }

    fn json_request(method: &str, uri: &str, body: impl Serialize) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&body).expect("request should serialize"),
            ))
            .expect("request should be valid")
    }

    #[tokio::test]
    async fn health_endpoint_reports_ok() {
        let app = app(ApiState::default());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response_json::<HealthResponse>(response).await,
            HealthResponse {
                status: "ok".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn create_room_endpoint_creates_room() {
        let app = app(ApiState::default());
        let request = CreateRoomRequest {
            id: RoomId(7),
            table_config: table_config(),
        };

        let response = app
            .oneshot(json_request("POST", "/rooms", request))
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response_json::<CreateRoomResponse>(response).await,
            CreateRoomResponse { id: RoomId(7) }
        );
    }

    #[tokio::test]
    async fn command_endpoint_dispatches_to_room_manager() {
        let app = app(ApiState::default());
        let create_request = CreateRoomRequest {
            id: RoomId(7),
            table_config: table_config(),
        };
        app.clone()
            .oneshot(json_request("POST", "/rooms", create_request))
            .await
            .expect("create room request should succeed");

        let command = RoomCommand::SitPlayer {
            id: PlayerId(1),
            display_name: "Ada".to_string(),
            seat: SeatIndex(0),
            buy_in: 1_000,
        };
        let response = app
            .oneshot(json_request("POST", "/rooms/7/commands", command))
            .await
            .expect("command request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let result = response_json::<RoomCommandResult>(response).await;
        assert!(result.events().is_empty());
        assert_eq!(result.snapshot().players().len(), 1);
    }

    #[tokio::test]
    async fn command_endpoint_returns_not_found_for_missing_room() {
        let app = app(ApiState::default());

        let response = app
            .oneshot(json_request(
                "POST",
                "/rooms/404/commands",
                RoomCommand::ApplyPlayerAction {
                    seat: SeatIndex(0),
                    action: PlayerAction::Check,
                },
            ))
            .await
            .expect("command request should succeed");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response_json::<ApiErrorBody>(response).await;
        assert!(body.error.contains("404"));
    }

    #[test]
    fn websocket_text_handler_returns_command_result_message() {
        let mut manager = RoomManager::new();
        manager
            .create_room(RoomId(7), table_config())
            .expect("room should be created");
        let state = ApiState::new(manager);
        let command = RoomCommand::SitPlayer {
            id: PlayerId(1),
            display_name: "Ada".to_string(),
            seat: SeatIndex(0),
            buy_in: 1_000,
        };
        let text = serde_json::to_string(&command).expect("command should serialize");

        let message = handle_room_socket_text(&state, RoomId(7), &text);

        let WebSocketServerMessage::CommandResult(result) = message else {
            panic!("expected command result websocket message");
        };
        assert!(result.events().is_empty());
        assert_eq!(result.snapshot().players().len(), 1);
    }

    #[test]
    fn websocket_text_handler_returns_error_for_invalid_json() {
        let state = ApiState::default();

        let message = handle_room_socket_text(&state, RoomId(7), "not json");

        let WebSocketServerMessage::Error(error) = message else {
            panic!("expected error websocket message");
        };
        assert!(error.error.contains("invalid room command JSON"));
    }
}

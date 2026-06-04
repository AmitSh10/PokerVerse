use std::{
    collections::HashMap,
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
use poker_engine::{GameSnapshot, TableConfig};
use poker_server::{
    RoomCommand, RoomCommandResult, RoomId, RoomManager, RoomManagerError, RoomSummary,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

const ROOM_BROADCAST_CAPACITY: usize = 128;

#[derive(Debug, Clone)]
pub struct ApiState {
    rooms: Arc<RwLock<RoomManager>>,
    room_channels: Arc<RwLock<HashMap<RoomId, broadcast::Sender<WebSocketServerMessage>>>>,
}

impl ApiState {
    pub fn new(room_manager: RoomManager) -> Self {
        Self {
            rooms: Arc::new(RwLock::new(room_manager)),
            room_channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn room_sender(
        &self,
        room_id: RoomId,
    ) -> Result<broadcast::Sender<WebSocketServerMessage>, ApiError> {
        let mut room_channels = self
            .room_channels
            .write()
            .map_err(|_| ApiError::StateLockPoisoned)?;

        Ok(room_channels
            .entry(room_id)
            .or_insert_with(|| {
                let (sender, _) = broadcast::channel(ROOM_BROADCAST_CAPACITY);
                sender
            })
            .clone())
    }

    fn subscribe_room(
        &self,
        room_id: RoomId,
    ) -> Result<broadcast::Receiver<WebSocketServerMessage>, ApiError> {
        Ok(self.room_sender(room_id)?.subscribe())
    }

    fn broadcast_room_message(&self, room_id: RoomId, message: WebSocketServerMessage) {
        let Ok(sender) = self.room_sender(room_id) else {
            return;
        };

        let _ = sender.send(message);
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
        .route("/rooms", get(list_rooms).post(create_room))
        .route("/rooms/{room_id}", get(get_room))
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
pub struct ListRoomsResponse {
    rooms: Vec<RoomSummary>,
}

impl ListRoomsResponse {
    pub fn rooms(&self) -> &[RoomSummary] {
        &self.rooms
    }
}

async fn list_rooms(State(state): State<ApiState>) -> Result<Json<ListRoomsResponse>, ApiError> {
    let rooms = state
        .rooms
        .read()
        .map_err(|_| ApiError::StateLockPoisoned)?;
    let summaries = rooms.room_summaries()?;

    Ok(Json(ListRoomsResponse { rooms: summaries }))
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
    drop(rooms);

    let _ = state.room_sender(request.id)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateRoomResponse { id: request.id }),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomDetailsResponse {
    summary: RoomSummary,
    snapshot: GameSnapshot,
}

impl RoomDetailsResponse {
    pub fn summary(&self) -> &RoomSummary {
        &self.summary
    }

    pub fn snapshot(&self) -> &GameSnapshot {
        &self.snapshot
    }
}

async fn get_room(
    Path(room_id): Path<u64>,
    State(state): State<ApiState>,
) -> Result<Json<RoomDetailsResponse>, ApiError> {
    let room_id = RoomId(room_id);
    let rooms = state
        .rooms
        .read()
        .map_err(|_| ApiError::StateLockPoisoned)?;
    let summary = rooms.room_summary(room_id)?;
    let snapshot = rooms.public_snapshot(room_id)?;

    Ok(Json(RoomDetailsResponse { summary, snapshot }))
}

async fn handle_room_command(
    Path(room_id): Path<u64>,
    State(state): State<ApiState>,
    Json(command): Json<RoomCommand>,
) -> Result<Json<RoomCommandResult>, ApiError> {
    let result = dispatch_room_command(&state, RoomId(room_id), command)?;
    state.broadcast_room_message(
        RoomId(room_id),
        WebSocketServerMessage::CommandResult(result.clone()),
    );

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
    let Ok(mut room_receiver) = state.subscribe_room(room_id) else {
        let _ = send_room_socket_message(
            &mut socket,
            &WebSocketServerMessage::Error(ApiErrorBody {
                error: "room broadcast channel unavailable".to_string(),
            }),
        )
        .await;
        return;
    };

    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(message) = message else {
                    break;
                };

                let Some(response) = handle_room_socket_message(&state, room_id, message) else {
                    break;
                };

                if matches!(response, WebSocketServerMessage::Ignored) {
                    continue;
                } else if matches!(response, WebSocketServerMessage::CommandResult(_)) {
                    state.broadcast_room_message(room_id, response);
                } else if send_room_socket_message(&mut socket, &response).await.is_err() {
                    break;
                }
            }
            message = room_receiver.recv() => {
                let response = match message {
                    Ok(message) => message,
                    Err(broadcast::error::RecvError::Lagged(_)) => WebSocketServerMessage::Error(ApiErrorBody {
                        error: "room websocket receiver lagged behind".to_string(),
                    }),
                    Err(broadcast::error::RecvError::Closed) => break,
                };

                if send_room_socket_message(&mut socket, &response).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn send_room_socket_message(
    socket: &mut WebSocket,
    response: &WebSocketServerMessage,
) -> Result<(), axum::Error> {
    let Ok(payload) = serde_json::to_string(response) else {
        return Ok(());
    };

    socket.send(Message::Text(payload.into())).await
}

fn handle_room_socket_message(
    state: &ApiState,
    room_id: RoomId,
    message: Result<Message, axum::Error>,
) -> Option<WebSocketServerMessage> {
    Some(match message {
        Ok(Message::Text(text)) => handle_room_socket_text(state, room_id, text.as_ref()),
        Ok(Message::Close(_)) => return None,
        Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
            return Some(WebSocketServerMessage::Ignored);
        }
        Ok(Message::Binary(_)) => WebSocketServerMessage::Error(ApiErrorBody {
            error: "expected text JSON room command".to_string(),
        }),
        Err(error) => WebSocketServerMessage::Error(ApiErrorBody {
            error: format!("websocket receive error: {error}"),
        }),
    })
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
    Ignored,
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
    async fn list_rooms_endpoint_returns_room_summaries() {
        let app = app(ApiState::default());
        for id in [RoomId(7), RoomId(3)] {
            app.clone()
                .oneshot(json_request(
                    "POST",
                    "/rooms",
                    CreateRoomRequest {
                        id,
                        table_config: table_config(),
                    },
                ))
                .await
                .expect("create room request should succeed");
        }

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/rooms")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json::<ListRoomsResponse>(response).await;
        assert_eq!(
            body.rooms().iter().map(RoomSummary::id).collect::<Vec<_>>(),
            vec![RoomId(3), RoomId(7)]
        );
    }

    #[tokio::test]
    async fn get_room_endpoint_returns_summary_and_snapshot() {
        let app = app(ApiState::default());
        app.clone()
            .oneshot(json_request(
                "POST",
                "/rooms",
                CreateRoomRequest {
                    id: RoomId(7),
                    table_config: table_config(),
                },
            ))
            .await
            .expect("create room request should succeed");
        app.clone()
            .oneshot(json_request(
                "POST",
                "/rooms/7/commands",
                RoomCommand::SitPlayer {
                    id: PlayerId(1),
                    display_name: "Ada".to_string(),
                    seat: SeatIndex(0),
                    buy_in: 1_000,
                },
            ))
            .await
            .expect("sit player request should succeed");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/rooms/7")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json::<RoomDetailsResponse>(response).await;
        assert_eq!(body.summary().id(), RoomId(7));
        assert_eq!(body.summary().seated_player_count(), 1);
        assert_eq!(body.snapshot().players().len(), 1);
    }

    #[tokio::test]
    async fn get_room_endpoint_returns_not_found_for_missing_room() {
        let app = app(ApiState::default());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/rooms/404")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response_json::<ApiErrorBody>(response).await;
        assert!(body.error.contains("404"));
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
    async fn command_endpoint_handles_leave_seat_command() {
        let app = app(ApiState::default());
        app.clone()
            .oneshot(json_request(
                "POST",
                "/rooms",
                CreateRoomRequest {
                    id: RoomId(7),
                    table_config: table_config(),
                },
            ))
            .await
            .expect("create room request should succeed");
        app.clone()
            .oneshot(json_request(
                "POST",
                "/rooms/7/commands",
                RoomCommand::SitPlayer {
                    id: PlayerId(1),
                    display_name: "Ada".to_string(),
                    seat: SeatIndex(0),
                    buy_in: 1_000,
                },
            ))
            .await
            .expect("sit player request should succeed");

        let response = app
            .oneshot(json_request(
                "POST",
                "/rooms/7/commands",
                RoomCommand::LeaveSeat { seat: SeatIndex(0) },
            ))
            .await
            .expect("leave seat request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let result = response_json::<RoomCommandResult>(response).await;
        assert!(result.events().is_empty());
        assert!(result.snapshot().players().is_empty());
    }

    #[tokio::test]
    async fn command_endpoint_broadcasts_result_to_room_subscribers() {
        let state = ApiState::default();
        let app = app(state.clone());
        let create_request = CreateRoomRequest {
            id: RoomId(7),
            table_config: table_config(),
        };
        app.clone()
            .oneshot(json_request("POST", "/rooms", create_request))
            .await
            .expect("create room request should succeed");
        let mut receiver = state
            .subscribe_room(RoomId(7))
            .expect("room subscription should be created");

        let command = RoomCommand::SitPlayer {
            id: PlayerId(1),
            display_name: "Ada".to_string(),
            seat: SeatIndex(0),
            buy_in: 1_000,
        };
        app.oneshot(json_request("POST", "/rooms/7/commands", command))
            .await
            .expect("command request should succeed");

        let message = receiver
            .try_recv()
            .expect("subscriber should receive command result");
        let WebSocketServerMessage::CommandResult(result) = message else {
            panic!("expected command result broadcast");
        };
        assert_eq!(result.snapshot().players().len(), 1);
    }

    #[tokio::test]
    async fn command_endpoint_only_broadcasts_to_matching_room() {
        let state = ApiState::default();
        let app = app(state.clone());
        for id in [RoomId(7), RoomId(8)] {
            app.clone()
                .oneshot(json_request(
                    "POST",
                    "/rooms",
                    CreateRoomRequest {
                        id,
                        table_config: table_config(),
                    },
                ))
                .await
                .expect("create room request should succeed");
        }
        let mut other_room_receiver = state
            .subscribe_room(RoomId(8))
            .expect("room subscription should be created");

        app.oneshot(json_request(
            "POST",
            "/rooms/7/commands",
            RoomCommand::SitPlayer {
                id: PlayerId(1),
                display_name: "Ada".to_string(),
                seat: SeatIndex(0),
                buy_in: 1_000,
            },
        ))
        .await
        .expect("command request should succeed");

        assert!(other_room_receiver.try_recv().is_err());
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

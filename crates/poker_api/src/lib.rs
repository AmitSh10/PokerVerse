use std::{
    collections::HashMap,
    env, fmt,
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
use poker_engine::{GameSnapshot, SeatIndex, TableConfig};
use poker_server::{
    RoomCommand, RoomCommandResult, RoomId, RoomManager, RoomManagerError, RoomSummary,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

const ROOM_BROADCAST_CAPACITY: usize = 128;
pub const API_ADDR_ENV: &str = "POKERVERSE_API_ADDR";

pub fn default_api_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 3000))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiConfig {
    addr: SocketAddr,
}

impl ApiConfig {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }

    pub fn from_env() -> Result<Self, ApiConfigError> {
        Self::from_addr_value(env::var(API_ADDR_ENV).ok())
    }

    pub fn from_addr_value(value: Option<impl AsRef<str>>) -> Result<Self, ApiConfigError> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let value = value.as_ref().trim();

        if value.is_empty() {
            return Ok(Self::default());
        }

        Ok(Self::new(value.parse().map_err(|source| {
            ApiConfigError::InvalidAddr {
                value: value.to_string(),
                source,
            }
        })?))
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self::new(default_api_addr())
    }
}

#[derive(Debug)]
pub enum ApiConfigError {
    InvalidAddr {
        value: String,
        source: std::net::AddrParseError,
    },
}

impl fmt::Display for ApiConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddr { value, source } => {
                write!(f, "invalid api bind address '{value}': {source}")
            }
        }
    }
}

impl std::error::Error for ApiConfigError {}

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

    fn remove_room_sender(&self, room_id: RoomId) -> Result<(), ApiError> {
        let mut room_channels = self
            .room_channels
            .write()
            .map_err(|_| ApiError::StateLockPoisoned)?;
        room_channels.remove(&room_id);

        Ok(())
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
        .route("/rooms/{room_id}", get(get_room).delete(close_room))
        .route(
            "/rooms/{room_id}/seats/{seat}/snapshot",
            get(get_private_snapshot),
        )
        .route("/rooms/{room_id}/commands", post(handle_room_command))
        .route("/rooms/{room_id}/ws", any(room_websocket))
        .layer(cors_layer())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn cors_layer() -> CorsLayer {
    CorsLayer::permissive()
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

async fn get_private_snapshot(
    Path((room_id, seat)): Path<(u64, u8)>,
    State(state): State<ApiState>,
) -> Result<Json<GameSnapshot>, ApiError> {
    let rooms = state
        .rooms
        .read()
        .map_err(|_| ApiError::StateLockPoisoned)?;
    let snapshot = rooms.private_snapshot_for(RoomId(room_id), SeatIndex(seat))?;

    Ok(Json(snapshot))
}

async fn close_room(
    Path(room_id): Path<u64>,
    State(state): State<ApiState>,
) -> Result<StatusCode, ApiError> {
    let room_id = RoomId(room_id);
    let mut rooms = state
        .rooms
        .write()
        .map_err(|_| ApiError::StateLockPoisoned)?;
    rooms.close_room(room_id)?;
    drop(rooms);

    state.remove_room_sender(room_id)?;

    Ok(StatusCode::NO_CONTENT)
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
            &WebSocketServerMessage::Error(ApiErrorBody::new(
                ApiErrorCode::InternalError,
                "room broadcast channel unavailable",
            )),
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
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        WebSocketServerMessage::Error(ApiErrorBody::new(
                            ApiErrorCode::WebSocketLagged,
                            "room websocket receiver lagged behind",
                        ))
                    }
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
        Ok(Message::Binary(_)) => WebSocketServerMessage::Error(ApiErrorBody::new(
            ApiErrorCode::InvalidWebSocketMessage,
            "expected text JSON room command",
        )),
        Err(error) => WebSocketServerMessage::Error(ApiErrorBody::new(
            ApiErrorCode::WebSocketReceiveError,
            format!("websocket receive error: {error}"),
        )),
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
            return WebSocketServerMessage::Error(ApiErrorBody::new(
                ApiErrorCode::InvalidRoomCommandJson,
                format!("invalid room command JSON: {error}"),
            ));
        }
    };

    match dispatch_room_command(state, room_id, command) {
        Ok(result) => WebSocketServerMessage::CommandResult(result),
        Err(error) => WebSocketServerMessage::Error(ApiErrorBody::from_api_error(error)),
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

        (status, Json(ApiErrorBody::from_api_error(self))).into_response()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    InternalError,
    RoomAlreadyExists,
    RoomNotFound,
    InvalidRoomCommand,
    InvalidRoomCommandJson,
    InvalidWebSocketMessage,
    WebSocketLagged,
    WebSocketReceiveError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    code: ApiErrorCode,
    error: String,
}

impl ApiErrorBody {
    pub fn new(code: ApiErrorCode, error: impl Into<String>) -> Self {
        Self {
            code,
            error: error.into(),
        }
    }

    pub fn from_api_error(error: ApiError) -> Self {
        let code = match &error {
            ApiError::StateLockPoisoned => ApiErrorCode::InternalError,
            ApiError::RoomManager(RoomManagerError::RoomAlreadyExists { .. }) => {
                ApiErrorCode::RoomAlreadyExists
            }
            ApiError::RoomManager(RoomManagerError::RoomNotFound { .. }) => {
                ApiErrorCode::RoomNotFound
            }
            ApiError::RoomManager(RoomManagerError::RoomLockPoisoned { .. }) => {
                ApiErrorCode::InternalError
            }
            ApiError::RoomManager(RoomManagerError::Room(_)) => ApiErrorCode::InvalidRoomCommand,
        };

        Self::new(code, error.to_string())
    }

    pub fn code(&self) -> ApiErrorCode {
        self.code
    }

    pub fn error(&self) -> &str {
        &self.error
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

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

    #[test]
    fn api_config_defaults_to_localhost_port_3000() {
        let config = ApiConfig::from_addr_value(None::<&str>).expect("config should parse");

        assert_eq!(config.addr(), default_api_addr());
    }

    #[test]
    fn api_config_uses_default_for_blank_addr_value() {
        let config = ApiConfig::from_addr_value(Some("   ")).expect("config should parse");

        assert_eq!(config.addr(), default_api_addr());
    }

    #[test]
    fn api_config_parses_custom_addr_value() {
        let config = ApiConfig::from_addr_value(Some("0.0.0.0:8080")).expect("config should parse");

        assert_eq!(
            config.addr(),
            "0.0.0.0:8080"
                .parse::<SocketAddr>()
                .expect("test address should parse")
        );
    }

    #[test]
    fn api_config_rejects_invalid_addr_value() {
        let error =
            ApiConfig::from_addr_value(Some("not an address")).expect_err("config should fail");

        assert!(error.to_string().contains("not an address"));
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
    async fn cors_preflight_allows_browser_api_requests() {
        let app = app(ApiState::default());

        let response = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/rooms")
                    .header(header::ORIGIN, "http://localhost:5173")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&"*".parse().expect("wildcard origin header should parse"))
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
    async fn create_room_endpoint_returns_conflict_for_duplicate_room() {
        let app = app(ApiState::default());
        let request = CreateRoomRequest {
            id: RoomId(7),
            table_config: table_config(),
        };
        app.clone()
            .oneshot(json_request("POST", "/rooms", request.clone()))
            .await
            .expect("first create room request should succeed");

        let response = app
            .oneshot(json_request("POST", "/rooms", request))
            .await
            .expect("duplicate create room request should succeed");

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = response_json::<ApiErrorBody>(response).await;
        assert_eq!(body.code(), ApiErrorCode::RoomAlreadyExists);
        assert!(body.error().contains("7"));
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
        assert_eq!(body.code(), ApiErrorCode::RoomNotFound);
        assert!(body.error().contains("404"));
    }

    #[tokio::test]
    async fn private_snapshot_endpoint_reveals_only_requested_seat_cards() {
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
        for (id, name, seat) in [
            (PlayerId(1), "Ada", SeatIndex(0)),
            (PlayerId(2), "Linus", SeatIndex(3)),
        ] {
            app.clone()
                .oneshot(json_request(
                    "POST",
                    "/rooms/7/commands",
                    RoomCommand::SitPlayer {
                        id,
                        display_name: name.to_string(),
                        seat,
                        buy_in: 1_000,
                    },
                ))
                .await
                .expect("sit player request should succeed");
        }
        app.clone()
            .oneshot(json_request(
                "POST",
                "/rooms/7/commands",
                RoomCommand::StartHand {
                    dealer_seat: SeatIndex(0),
                },
            ))
            .await
            .expect("start hand request should succeed");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/rooms/7/seats/0/snapshot")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let snapshot = response_json::<GameSnapshot>(response).await;
        let viewer = snapshot
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(0))
            .expect("viewer should be in snapshot");
        let other = snapshot
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(3))
            .expect("other player should be in snapshot");
        assert_eq!(
            viewer
                .visible_hole_cards()
                .expect("viewer cards should be visible")
                .len(),
            2
        );
        assert!(other.visible_hole_cards().is_none());
    }

    #[tokio::test]
    async fn private_snapshot_endpoint_rejects_empty_seat() {
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

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/rooms/7/seats/5/snapshot")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response_json::<ApiErrorBody>(response).await;
        assert_eq!(body.code(), ApiErrorCode::InvalidRoomCommand);
        assert!(body.error().contains("seat 5 is empty"));
    }

    #[tokio::test]
    async fn close_room_endpoint_removes_room_from_lobby() {
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

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/rooms/7")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("close room request should succeed");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/rooms")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("list rooms request should succeed");
        let body = response_json::<ListRoomsResponse>(response).await;
        assert!(body.rooms().is_empty());
    }

    #[tokio::test]
    async fn close_room_endpoint_returns_not_found_for_missing_room() {
        let app = app(ApiState::default());

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/rooms/404")
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("close room request should succeed");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response_json::<ApiErrorBody>(response).await;
        assert_eq!(body.code(), ApiErrorCode::RoomNotFound);
        assert!(body.error().contains("404"));
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
    async fn command_endpoint_handles_sit_out_and_sit_in_commands() {
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
            .clone()
            .oneshot(json_request(
                "POST",
                "/rooms/7/commands",
                RoomCommand::SitOut { seat: SeatIndex(0) },
            ))
            .await
            .expect("sit out request should succeed");
        let result = response_json::<RoomCommandResult>(response).await;
        assert_eq!(
            result.snapshot().players()[0].status(),
            poker_engine::PlayerStatus::SittingOut
        );

        let response = app
            .oneshot(json_request(
                "POST",
                "/rooms/7/commands",
                RoomCommand::SitIn { seat: SeatIndex(0) },
            ))
            .await
            .expect("sit in request should succeed");
        let result = response_json::<RoomCommandResult>(response).await;
        assert_eq!(
            result.snapshot().players()[0].status(),
            poker_engine::PlayerStatus::Active
        );
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
        assert_eq!(body.code(), ApiErrorCode::RoomNotFound);
        assert!(body.error().contains("404"));
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
        assert_eq!(error.code(), ApiErrorCode::InvalidRoomCommandJson);
        assert!(error.error().contains("invalid room command JSON"));
    }
}

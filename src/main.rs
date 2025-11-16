use std::{fs, net::SocketAddr, path::PathBuf, sync::Arc};
use std::{fs, net::SocketAddr, sync::Arc};

use argon2::{
    password_hash::{
        Error as PasswordHashError, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    body::Body,
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, Request, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use tokio::net::TcpListener;
use tower_cookies::{Cookie, CookieManagerLayer, Cookies};
use tower_http::services::{ServeDir, ServeFile};
use tokio::fs as tokio_fs;
use tower::ServiceExt;
use tower_cookies::{Cookie, CookieManagerLayer, Cookies};
use tower_http::services::ServeDir;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

const SESSION_COOKIE: &str = "session_id";
const SESSION_TTL_DAYS: i64 = 7;
const CATEGORIES: [&str; 5] = ["扩列", "吐槽", "表白", "提问", "其它"];
const LATEST_CATEGORY: &str = "最新";
const DEFAULT_CATEGORY: &str = "其它";
const STATIC_DIR: &str = "frontend/dist";
const FRONTEND_ENTRY: &str = "index.html";

type SharedState = Arc<AppState>;
type ApiResult<T> = std::result::Result<T, ApiError>;

type SharedState = Arc<AppState>;
type ApiResult<T> = Result<T, ApiError>;

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    config: AppConfig,
}

#[derive(Clone, Deserialize)]
struct AppConfig {
    server: ServerSection,
    #[serde(default)]
    admins: AdminSection,
}

impl AppConfig {
    fn is_admin(&self, uid: &str) -> bool {
        self.admins.uids.iter().any(|candidate| candidate == uid)
    }
}

#[derive(Clone, Deserialize)]
struct ServerSection {
    addr: String,
}

#[derive(Clone, Deserialize)]
struct AdminSection {
    #[serde(default)]
    uids: Vec<String>,
}

impl Default for AdminSection {
    fn default() -> Self {
        Self { uids: Vec::new() }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = load_config()?;
    let pool = SqlitePool::connect("sqlite:talk_wall.db").await?;
    init_db(&pool).await?;

    let state = Arc::new(AppState {
        db: pool,
        config: config.clone(),
    });

    if fs::metadata(STATIC_DIR).is_err() {
        warn!("{STATIC_DIR} 不存在，运行 `npm install && npm run build` 以构建 Svelte 前端");
    } else {
        let entry = PathBuf::from(STATIC_DIR).join(FRONTEND_ENTRY);
        if fs::metadata(&entry).is_err() {
            warn!(
                "未找到 {}，请确认 `npm run build` 是否成功并输出入口文件",
                entry.display()
            );
        }
    }

    let static_service = ServeDir::new(STATIC_DIR).not_found_service(ServeFile::new(
        PathBuf::from(STATIC_DIR).join(FRONTEND_ENTRY),
    ));

    let static_dir = "frontend/dist";
    if fs::metadata(static_dir).is_err() {
        warn!("{static_dir} 不存在，运行 `npm install && npm run build` 以构建 Svelte 前端");
    }

    let app = Router::new()
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(me).patch(update_profile))
        .route("/api/me/posts", get(list_my_posts))
        .route("/api/me/password", post(change_password))
        .route("/api/users/:uid", get(get_user_profile))
        .route("/api/posts", post(create_post).get(list_posts))
        .route("/api/posts/:post_id", get(get_post))
        .route("/api/posts/:post_id/comments", post(create_comment))
        .route("/api/admin/posts/:post_id", delete(delete_post))
        .with_state(state)
        .layer(CookieManagerLayer::new())
        .fallback_service(static_service);

    let addr: SocketAddr = config.server.addr.parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
        .fallback(static_handler);

    let addr: SocketAddr = config.server.addr.parse()?;
    info!("listening on {addr}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn register(
    State(state): State<SharedState>,
    Json(payload): Json<RegisterPayload>,
) -> ApiResult<impl IntoResponse> {
    if payload.username.trim().is_empty() {
        return Err(ApiError::Validation("用户名不能为空".into()));
    }
    if payload.qq.trim().is_empty() {
        return Err(ApiError::Validation("QQ号不能为空".into()));
    }
    if payload.password.len() < 6 {
        return Err(ApiError::Validation("密码至少需要6位".into()));
    }

    let hashed = hash_password(&payload.password)?;
    let now = now_iso();
    let uid = Uuid::new_v4().to_string();

    let result = sqlx::query(
        r#"INSERT INTO users (uid, username, qq, password_hash, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5)"#,
    )
    .bind(uid)
    .bind(payload.username.trim())
    .bind(payload.qq.trim())
    .bind(hashed)
    .bind(now)
    .execute(&state.db)
    .await;

    if let Err(err) = result {
        if is_unique_violation(&err) {
            return Err(ApiError::Conflict("用户名已存在".into()));
        }
        return Err(ApiError::from(err));
    }

    Ok((
        StatusCode::CREATED,
        Json(MessageResponse {
            message: "注册成功".into(),
        }),
    ))
}

async fn login(
    State(state): State<SharedState>,
    cookies: Cookies,
    Json(payload): Json<LoginPayload>,
) -> ApiResult<Json<UserResponse>> {
    let user = sqlx::query_as::<_, DbUser>(
        "SELECT id, uid, username, qq, password_hash FROM users WHERE username = ?1",
    )
    .bind(payload.username.trim())
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::Unauthorized)?;

    if !verify_password(&user.password_hash, &payload.password)? {
        return Err(ApiError::Unauthorized);
    }

    let session_id = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc();
    let expires_at = now + Duration::days(SESSION_TTL_DAYS);

    sqlx::query(
        r#"INSERT INTO sessions (id, user_id, created_at, expires_at)
           VALUES (?1, ?2, ?3, ?4)"#,
    )
    .bind(&session_id)
    .bind(user.id)
    .bind(now.format(&Rfc3339).unwrap())
    .bind(expires_at.format(&Rfc3339).unwrap())
    .execute(&state.db)
    .await?;

    let mut cookie = Cookie::new(SESSION_COOKIE, session_id);
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_max_age(Duration::days(SESSION_TTL_DAYS));
    cookies.add(cookie);

    let is_admin = state.config.is_admin(&user.uid);

    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        qq: user.qq,
        uid: user.uid,
        is_admin,
    }))
}

async fn logout(
    State(state): State<SharedState>,
    cookies: Cookies,
) -> ApiResult<impl IntoResponse> {
    if let Some(cookie) = cookies.get(SESSION_COOKIE) {
        let token = cookie.value().to_string();
        sqlx::query("DELETE FROM sessions WHERE id = ?1")
            .bind(&token)
            .execute(&state.db)
            .await?;

        let mut expired = Cookie::named(SESSION_COOKIE.to_string());
        expired.set_path("/");
        cookies.remove(expired);
    }

    Ok(Json(MessageResponse {
        message: "退出成功".into(),
    }))
}

async fn me(State(state): State<SharedState>, cookies: Cookies) -> ApiResult<Json<UserResponse>> {
    let user = authenticate(&state, &cookies).await?;
    let is_admin = state.config.is_admin(&user.uid);
    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        qq: user.qq,
        uid: user.uid,
        is_admin,
    }))
}

async fn create_post(
    State(state): State<SharedState>,
    cookies: Cookies,
    Json(payload): Json<CreatePostPayload>,
) -> ApiResult<impl IntoResponse> {
    if payload.title.trim().is_empty() {
        return Err(ApiError::Validation("标题不能为空".into()));
    }
    if payload.content.trim().is_empty() {
        return Err(ApiError::Validation("内容不能为空".into()));
    }

    let user = authenticate(&state, &cookies).await?;
    let now = now_iso();
    let anonymous = payload.anonymous.unwrap_or(false);
    let category = normalize_post_category(payload.category)?;

    sqlx::query(
        r#"INSERT INTO posts (user_id, title, content, category, is_anonymous, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
    )
    .bind(user.id)
    .bind(payload.title.trim())
    .bind(payload.content.trim())
    .bind(&category)
    .bind(anonymous)
    .bind(now)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(MessageResponse {
            message: "发布成功".into(),
        }),
    ))
}

async fn list_posts(
    State(state): State<SharedState>,
    cookies: Cookies,
    Query(query): Query<PostListQuery>,
) -> ApiResult<Json<Vec<PostSummary>>> {
    authenticate(&state, &cookies).await?;
    let category_filter = normalize_query_category(query.category)?;

    let posts = sqlx::query_as::<_, DbPost>(
        r#"SELECT p.id, p.user_id, p.title, p.content, p.category, p.is_anonymous, p.created_at,
                  u.username, u.qq, u.uid
           FROM posts p
           LEFT JOIN users u ON p.user_id = u.id
           WHERE (?1 IS NULL OR p.category = ?1)
           ORDER BY p.created_at DESC"#,
    )
    .bind(category_filter)
    .fetch_all(&state.db)
    .await?;
    let response = posts.into_iter().map(PostSummary::from).collect();

    Ok(Json(response))
}

async fn create_comment(
    State(state): State<SharedState>,
    cookies: Cookies,
    Path(post_id): Path<i64>,
    Json(payload): Json<CreateCommentPayload>,
) -> ApiResult<impl IntoResponse> {
    if payload.content.trim().is_empty() {
        return Err(ApiError::Validation("内容不能为空".into()));
    }

    let user = authenticate(&state, &cookies).await?;
    let now = now_iso();
    let anonymous = payload.anonymous.unwrap_or(false);

    sqlx::query(
        r#"INSERT INTO comments (post_id, user_id, content, is_anonymous, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5)"#,
    )
    .bind(post_id)
    .bind(user.id)
    .bind(payload.content.trim())
    .bind(anonymous)
    .bind(now)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(MessageResponse {
            message: "评论成功".into(),
        }),
    ))
}

async fn get_post(
    State(state): State<SharedState>,
    cookies: Cookies,
    Path(post_id): Path<i64>,
) -> ApiResult<Json<PostDetailResponse>> {
    authenticate(&state, &cookies).await?;
    let post = load_post(&state, post_id).await?;
    let comments = fetch_comments(&state, post.id).await?;
    Ok(Json(PostDetailResponse::from_parts(post, comments)))
}

async fn list_my_posts(
    State(state): State<SharedState>,
    cookies: Cookies,
) -> ApiResult<Json<Vec<PostSummary>>> {
    let user = authenticate(&state, &cookies).await?;
    let posts = fetch_posts_for_user(&state, user.id, true).await?;
    Ok(Json(posts))
}

async fn get_user_profile(
    State(state): State<SharedState>,
    cookies: Cookies,
    Path(uid): Path<String>,
) -> ApiResult<Json<UserProfileResponse>> {
    authenticate(&state, &cookies).await?;
    let user = find_user_by_uid(&state, &uid).await?;
    let posts = fetch_posts_for_user(&state, user.id, false).await?;
    Ok(Json(UserProfileResponse {
        username: user.username,
        qq: user.qq,
        uid: user.uid,
        joined_at: user.created_at,
        posts,
    }))
}

async fn update_profile(
    State(state): State<SharedState>,
    cookies: Cookies,
    Json(payload): Json<UpdateProfilePayload>,
) -> ApiResult<Json<UserResponse>> {
    let mut user = authenticate(&state, &cookies).await?;
    let new_username = payload
        .username
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| user.username.clone());
    let new_qq = payload
        .qq
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| user.qq.clone());

    if new_username == user.username && new_qq == user.qq {
        return Err(ApiError::Validation("没有需要更新的内容".into()));
    }

    let result = sqlx::query("UPDATE users SET username = ?1, qq = ?2 WHERE id = ?3")
        .bind(&new_username)
        .bind(&new_qq)
        .bind(user.id)
        .execute(&state.db)
        .await;

    if let Err(err) = result {
        if is_unique_violation(&err) {
            return Err(ApiError::Conflict("用户名已存在".into()));
        }
        return Err(ApiError::from(err));
    }

    user.username = new_username;
    user.qq = new_qq;

    let is_admin = state.config.is_admin(&user.uid);
    Ok(Json(UserResponse {
        id: user.id,
        username: user.username,
        qq: user.qq,
        uid: user.uid,
        is_admin,
    }))
}

async fn change_password(
    State(state): State<SharedState>,
    cookies: Cookies,
    Json(payload): Json<ChangePasswordPayload>,
) -> ApiResult<Json<MessageResponse>> {
    if payload.new_password.len() < 6 {
        return Err(ApiError::Validation("新密码至少需要6位".into()));
    }

    let user = authenticate(&state, &cookies).await?;
    let current_hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?1")
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?;

    let Some(current_hash) = current_hash else {
        return Err(ApiError::Internal("用户不存在".into()));
    };

    if !verify_password(&current_hash, &payload.current_password)? {
        return Err(ApiError::Validation("原密码错误".into()));
    }

    let new_hash = hash_password(&payload.new_password)?;
    sqlx::query("UPDATE users SET password_hash = ?1 WHERE id = ?2")
        .bind(new_hash)
        .bind(user.id)
        .execute(&state.db)
        .await?;

    Ok(Json(MessageResponse {
        message: "密码修改成功".into(),
    }))
}

async fn fetch_comments(state: &SharedState, post_id: i64) -> ApiResult<Vec<CommentResponse>> {
    let rows = sqlx::query_as::<_, DbComment>(
        r#"SELECT c.id, c.content, c.is_anonymous, c.created_at, u.username, u.qq, u.uid
           FROM comments c
           LEFT JOIN users u ON c.user_id = u.id
           WHERE c.post_id = ?1
           ORDER BY c.created_at ASC"#,
    )
    .bind(post_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows.into_iter().map(CommentResponse::from).collect())
}

async fn load_post(state: &SharedState, post_id: i64) -> ApiResult<DbPost> {
    sqlx::query_as::<_, DbPost>(
        r#"SELECT p.id, p.user_id, p.title, p.content, p.category, p.is_anonymous, p.created_at,
                  u.username, u.qq, u.uid
           FROM posts p
           LEFT JOIN users u ON p.user_id = u.id
           WHERE p.id = ?1"#,
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)
}

async fn fetch_posts_for_user(
    state: &SharedState,
    user_id: i64,
    include_anonymous: bool,
) -> ApiResult<Vec<PostSummary>> {
    let rows = sqlx::query_as::<_, DbPost>(
        r#"SELECT p.id, p.user_id, p.title, p.content, p.category, p.is_anonymous, p.created_at,
                  u.username, u.qq, u.uid
           FROM posts p
           LEFT JOIN users u ON p.user_id = u.id
           WHERE p.user_id = ?1 AND (?2 = 1 OR p.is_anonymous = 0)
           ORDER BY p.created_at DESC"#,
    )
    .bind(user_id)
    .bind(bool_to_int(include_anonymous))
    .fetch_all(&state.db)
    .await?;

    Ok(rows.into_iter().map(PostSummary::from).collect())
}

async fn find_user_by_uid(state: &SharedState, uid: &str) -> ApiResult<DbPublicUser> {
    sqlx::query_as::<_, DbPublicUser>(
        "SELECT id, uid, username, qq, created_at FROM users WHERE uid = ?1",
    )
    .bind(uid)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)
}

fn bool_to_int(flag: bool) -> i64 {
    if flag {
        1
    } else {
        0
    }
}

fn normalize_post_category(category: Option<String>) -> Result<String, ApiError> {
    let value = category.unwrap_or_else(|| DEFAULT_CATEGORY.to_string());
    let trimmed = value.trim();
    let normalized = if trimmed.is_empty() {
        DEFAULT_CATEGORY
    } else {
        trimmed
    };
    if CATEGORIES.contains(&normalized) {
        Ok(normalized.to_string())
    } else {
        Err(ApiError::Validation("请选择有效的分区".into()))
    }
}

fn normalize_query_category(category: Option<String>) -> Result<Option<String>, ApiError> {
    if let Some(raw) = category {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == LATEST_CATEGORY {
            return Ok(None);
        }
        if CATEGORIES.contains(&trimmed) {
            return Ok(Some(trimmed.to_string()));
        }
        return Err(ApiError::Validation("未知的分区筛选".into()));
    }
    Ok(None)
}

async fn ensure_column(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), sqlx::Error> {
    let query = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
    match sqlx::query(&query).execute(pool).await {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(err)) if err.message().contains("duplicate column name") => {
            Ok(())
        }
        Err(err) => Err(err),
    }
}

async fn delete_post(
    State(state): State<SharedState>,
    cookies: Cookies,
    Path(post_id): Path<i64>,
) -> ApiResult<Json<MessageResponse>> {
    let user = authenticate(&state, &cookies).await?;
    if !state.config.is_admin(&user.uid) {
        return Err(ApiError::Forbidden);
    }

    let result = sqlx::query("DELETE FROM posts WHERE id = ?1")
        .bind(post_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(Json(MessageResponse {
        message: "帖子已删除".into(),
    }))
}

async fn static_handler(uri: Uri) -> Response {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();

    match ServeDir::new("frontend/dist").oneshot(req).await {
        Ok(res) => {
            if res.status() == StatusCode::NOT_FOUND {
                serve_index().await
            } else {
                res
            }
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("静态资源加载失败: {err}"),
        )
            .into_response(),
    }
}

async fn serve_index() -> Response {
    match tokio_fs::read("frontend/dist/index.html").await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(bytes))
            .unwrap_or_else(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("响应构建失败: {err}"),
                )
                    .into_response()
            }),
        Err(err) => (StatusCode::NOT_FOUND, format!("静态资源不存在: {err}")).into_response(),
    }
}

fn hash_password(password: &str) -> ApiResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(ApiError::from)
}

fn verify_password(hash: &str, password: &str) -> ApiResult<bool> {
    let parsed = PasswordHash::new(hash).map_err(ApiError::from)?;
    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(_) => Ok(true),
        Err(PasswordHashError::Password) => Ok(false),
        Err(err) => Err(ApiError::from(err)),
    }
}

async fn authenticate(state: &SharedState, cookies: &Cookies) -> ApiResult<AuthedUser> {
    let Some(cookie) = cookies.get(SESSION_COOKIE) else {
        return Err(ApiError::Unauthorized);
    };
    let token = cookie.value().to_string();

    let session = sqlx::query_as::<_, DbSession>(
        r#"SELECT s.user_id, s.expires_at, u.username, u.qq, u.uid
           FROM sessions s
           JOIN users u ON u.id = s.user_id
           WHERE s.id = ?1"#,
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::Unauthorized)?;

    let expires = OffsetDateTime::parse(&session.expires_at, &Rfc3339)
        .map_err(|err| ApiError::Internal(format!("时间解析失败: {err}")))?;
    if expires <= OffsetDateTime::now_utc() {
        sqlx::query("DELETE FROM sessions WHERE id = ?1")
            .bind(&token)
            .execute(&state.db)
            .await?;
        return Err(ApiError::Unauthorized);
    }

    Ok(AuthedUser {
        id: session.user_id,
        username: session.username,
        qq: session.qq,
        uid: session.uid,
    })
}

fn now_iso() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string("config.toml")?;
    let config: AppConfig = toml::from_str(&contents)?;
    Ok(config)
}

async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uid TEXT NOT NULL UNIQUE,
            username TEXT NOT NULL UNIQUE,
            qq TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL
        );"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            user_id INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        );"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS posts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT '其它',
            is_anonymous INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        );"#,
    )
    .execute(pool)
    .await?;

    ensure_column(pool, "posts", "title", "TEXT NOT NULL DEFAULT ''").await?;
    ensure_column(pool, "posts", "category", "TEXT NOT NULL DEFAULT '其它'").await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS comments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            post_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            content TEXT NOT NULL,
            is_anonymous INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(post_id) REFERENCES posts(id) ON DELETE CASCADE,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        );"#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = err {
        return db_err.message().contains("UNIQUE");
    }
    false
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),
    #[error("未授权")]
    Unauthorized,
    #[error("禁止访问")]
    Forbidden,
    #[error("请求参数错误: {0}")]
    Validation(String),
    #[error("冲突: {0}")]
    Conflict(String),
    #[error("资源不存在")]
    NotFound,
    #[error("密码处理失败: {0}")]
    PasswordHash(String),
    #[error("服务器内部错误: {0}")]
    Internal(String),
}

impl From<PasswordHashError> for ApiError {
    fn from(err: PasswordHashError) -> Self {
        ApiError::PasswordHash(err.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            ApiError::Validation(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let body = Json(ErrorResponse {
            message: self.to_string(),
        });

        (status, body).into_response()
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}

#[derive(Serialize)]
struct MessageResponse {
    message: String,
}

#[derive(Deserialize)]
struct RegisterPayload {
    username: String,
    qq: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginPayload {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct CreatePostPayload {
    title: String,
    content: String,
    category: Option<String>,
    anonymous: Option<bool>,
}

#[derive(Deserialize)]
struct CreateCommentPayload {
    content: String,
    anonymous: Option<bool>,
}

#[derive(Deserialize)]
struct PostListQuery {
    category: Option<String>,
}

#[derive(Deserialize)]
struct UpdateProfilePayload {
    username: Option<String>,
    qq: Option<String>,
}

#[derive(Deserialize)]
struct ChangePasswordPayload {
    current_password: String,
    new_password: String,
}

#[derive(Serialize)]
struct UserResponse {
    id: i64,
    username: String,
    qq: String,
    uid: String,
    is_admin: bool,
}

#[derive(Serialize)]
struct UserProfileResponse {
    username: String,
    qq: String,
    uid: String,
    joined_at: String,
    posts: Vec<PostSummary>,
}

#[derive(Serialize)]
struct PostSummary {
    id: i64,
    title: String,
    content: String,
    category: String,
    created_at: String,
    anonymous: bool,
    author: Option<AuthorInfo>,
}

#[derive(Serialize)]
struct PostDetailResponse {
    id: i64,
    title: String,
    content: String,
    category: String,
    created_at: String,
    anonymous: bool,
    author: Option<AuthorInfo>,
    comments: Vec<CommentResponse>,
}

#[derive(Serialize)]
struct CommentResponse {
    id: i64,
    content: String,
    created_at: String,
    anonymous: bool,
    author: Option<AuthorInfo>,
}

#[derive(Serialize)]
struct AuthorInfo {
    username: String,
    qq: String,
    uid: String,
}

struct AuthedUser {
    id: i64,
    username: String,
    qq: String,
    uid: String,
}

#[derive(FromRow)]
struct DbUser {
    id: i64,
    uid: String,
    username: String,
    qq: String,
    password_hash: String,
}

#[derive(FromRow)]
struct DbPost {
    id: i64,
    user_id: i64,
    title: String,
    content: String,
    category: String,
    is_anonymous: bool,
    created_at: String,
    username: Option<String>,
    qq: Option<String>,
    uid: Option<String>,
}

#[derive(FromRow)]
struct DbComment {
    id: i64,
    content: String,
    is_anonymous: bool,
    created_at: String,
    username: Option<String>,
    qq: Option<String>,
    uid: Option<String>,
}

#[derive(FromRow)]
struct DbPublicUser {
    id: i64,
    uid: String,
    username: String,
    qq: String,
    created_at: String,
}

#[derive(FromRow)]
struct DbSession {
    user_id: i64,
    expires_at: String,
    username: String,
    qq: String,
    uid: String,
}

impl From<DbPost> for PostSummary {
    fn from(value: DbPost) -> Self {
        let author = if !value.is_anonymous {
            match (value.username.clone(), value.qq.clone(), value.uid.clone()) {
                (Some(username), Some(qq), Some(uid)) => Some(AuthorInfo { username, qq, uid }),
                _ => None,
            }
        } else {
            None
        };
        Self {
            id: value.id,
            title: value.title,
            content: value.content,
            category: value.category,
            created_at: value.created_at,
            anonymous: value.is_anonymous,
            author,
        }
    }
}

impl PostDetailResponse {
    fn from_parts(post: DbPost, comments: Vec<CommentResponse>) -> Self {
        let summary: PostSummary = post.into();
        Self {
            id: summary.id,
            title: summary.title,
            content: summary.content,
            category: summary.category,
            created_at: summary.created_at,
            anonymous: summary.anonymous,
            author: summary.author,
            comments,
        }
    }
}

impl From<DbComment> for CommentResponse {
    fn from(value: DbComment) -> Self {
        let author = if !value.is_anonymous {
            match (value.username.clone(), value.qq.clone(), value.uid.clone()) {
                (Some(username), Some(qq), Some(uid)) => Some(AuthorInfo { username, qq, uid }),
                _ => None,
            }
        } else {
            None
        };
        Self {
            id: value.id,
            content: value.content,
            created_at: value.created_at,
            anonymous: value.is_anonymous,
            author,
        }
    }
}

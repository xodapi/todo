use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Role {
    Admin,
    Expert,
    Engineer,
    Viewer,
    Manager, // Added Manager role
    Analyst, // Added Analyst role
}

impl Role {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Admin => "admin",
            Self::Expert => "expert",
            Self::Engineer => "engineer",
            Self::Viewer => "viewer",
            Self::Manager => "manager", // Added mapping for Manager
            Self::Analyst => "analyst", // Added mapping for Analyst
        }
    }

    pub fn can_approve(&self) -> bool {
        matches!(self, Self::Admin | Self::Expert)
    }
}

impl std::str::FromStr for Role {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "admin" => Ok(Self::Admin),
            "expert" => Ok(Self::Expert),
            "engineer" => Ok(Self::Engineer),
            "manager" => Ok(Self::Manager),
            "analyst" => Ok(Self::Analyst),
            _ => Ok(Self::Viewer),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: Role,
    pub full_name: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub source: Option<String>,
    pub status: String,
    pub priority: Option<f32>,
    pub impact: i32,
    pub effort: i32,
    pub is_urgent: bool,
    pub is_important: bool,
    pub approved_by: Option<i64>,
    pub assigned_to: Option<i64>,
    pub created_by: i64,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub deadline: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub remember_me: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub role: String,
    pub username: String,
    pub user_id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub is_urgent: bool,
    pub is_important: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub status: Option<String>,
    pub assigned_to: Option<i64>,
    pub is_urgent: Option<bool>,
    pub is_important: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StartTimerRequest {
    pub task_id: Option<i64>,
    pub category: i32,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub id: i64,
    pub user_id: i64,
    pub username: String,
    pub task_id: Option<i64>,
    pub body: String,
    pub sent_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub task_id: Option<i64>,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}

impl ApiError {
    pub fn new(msg: &str) -> Self {
        Self {
            error: msg.to_string(),
        }
    }
    pub fn json(msg: &str) -> String {
        serde_json::to_string(&Self::new(msg)).unwrap_or_default()
    }
}

// Windows Activity Monitoring Types
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WindowsActivity {
    pub id: i64,
    pub user_id: i64,
    pub process_name: String,
    pub window_title: String,
    pub started_at: String,
    pub duration_s: i64,
    pub is_private: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InputMetrics {
    pub id: i64,
    pub user_id: i64,
    pub key_count: i64,
    pub mouse_distance_px: i64,
    pub measured_at: String,
}
// Pulse & Reporting
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PulseSettings {
    pub interval_min: i64,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PulseQuestion {
    pub id: i64,
    pub user_id: i64,
    pub asked_at: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JournalEntry {
    pub id: i64,
    pub user_id: i64,
    pub username: String,
    pub event_type: String,
    pub task_id: Option<i64>,
    pub task_title: Option<String>,
    pub detail: String,
    pub duration_s: Option<i64>,
    pub category: Option<i32>,
    pub happened_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReflectionAnswer {
    pub id: i64,
    pub user_id: i64,
    pub question: String,
    pub answer: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitReflectionRequest {
    pub question: String,
    pub answer: String,
}

// --- Knowledge Base ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KnowledgeNote {
    pub id: i64,
    pub user_id: i64,
    pub parent_id: Option<i64>,
    pub title: String,
    pub content: String,
    pub aliases: String,
    pub tags: Vec<String>,
    pub is_archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NoteTag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NoteLink {
    pub source_id: i64,
    pub target_id: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KbGraphData {
    pub nodes: Vec<KbNode>,
    pub edges: Vec<KbEdge>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KbNode {
    pub id: i64,
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KbEdge {
    pub from: i64,
    pub to: i64,
}

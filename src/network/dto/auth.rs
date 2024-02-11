use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct AuthSuccessResponse {
    pub token: String,
}

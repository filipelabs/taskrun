//! HTTP request handlers.

mod enrollment;
mod events;
mod health;
mod responses_openai;
mod workers;

pub use enrollment::enroll;
pub use events::{get_task_events, get_task_output};
pub use health::{health_check, metrics_handler};
pub use responses_openai::create_response;
pub use workers::{list_workers_html, list_workers_json};

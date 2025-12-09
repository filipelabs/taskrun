//! View implementations.

mod workers;
mod tasks;
mod logs;
mod run_detail;
pub mod dialogs;

pub use workers::render_workers_view;
pub use tasks::render_tasks_view;
pub use logs::render_logs_view;
pub use run_detail::render_run_detail_view;

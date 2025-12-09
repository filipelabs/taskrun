//! View implementations.

pub mod dialogs;
mod logs;
mod run_detail;
mod tasks;
mod workers;

pub use logs::render_logs_view;
pub use run_detail::render_run_detail_view;
pub use tasks::render_tasks_view;
pub use workers::render_workers_view;

//! Trusted, reusable native system models and adapters.
//! The service layer must never depend on GTK, WebKit or shell widget state.
pub mod config_layers;
pub mod display_manager;
pub mod notification_queue;
pub mod system_metrics;

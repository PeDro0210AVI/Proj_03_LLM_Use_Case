mod anomalies;
mod clients;
mod finance;
mod inventory;
mod product;
mod project_states;
mod sales;
mod summary;
mod teams;

pub use anomalies::get_anomalies;
pub use clients::get_customer_metrics;
pub use finance::get_financial_status;
pub use inventory::get_inventory_levels;
pub use product::get_product_health;
pub use project_states::get_project_status;
pub use sales::{get_sales_metrics, get_top_products};
pub use summary::generate_executive_summary;
pub use teams::get_team_metrics;

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_nodes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_id: i64,
    #[sea_orm(indexed)]
    pub parent_id: i64,
    #[sea_orm(indexed)]
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub etag: Option<String>,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

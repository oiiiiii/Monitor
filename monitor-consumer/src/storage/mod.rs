//! 存储层
//!
//! 负责数据持久化：
//! - TDengine：波形数据（时序数据库）
//! - PostgreSQL：生命体征、报警事件、配置数据（关系型数据库）
//!
//! 遵循项目规范：
//! - 波形批量写入，禁止单条循环插入
//! - 生命体征、报警按时间分区
//! - 床位+时间联合索引

pub mod tdengine;
pub mod postgres;

pub use tdengine::TdEngineStore;
pub use postgres::PostgresStore;

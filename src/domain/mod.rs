//! Modelo de dominio: tipos puros, sin brókers ni transporte.

pub mod account;
pub mod asset;
pub mod candle;
pub mod error;
pub mod event;
pub mod operation;
pub mod payout;
pub mod platform;
pub mod position;

pub use account::{AccountBalance, AccountType, Balance};
pub use asset::{Asset, AssetClass, AssetExtras, MarketStatus, ScheduleWindow};
pub use candle::{Candle, CandleRequest, CandleSubscribe};
pub use error::DomainError;
pub use event::{Event, PriceTick};
pub use operation::{
    BuyBinaryRequest, BuyBlitzRequest, BuyDigitalRequest, Direction, Operation, OperationResult,
    OperationStatus, OptionType, OrderId, RolloverParams,
};
pub use payout::{Payout, StrikeList, StrikeQuote};
pub use platform::{
    AssetProfile, CommissionChange, FinancialInfo, LeaderboardEntry, LiveDeal, TechnicalIndicators,
    TopAsset,
};
pub use position::{Position, PositionHistory, Side};

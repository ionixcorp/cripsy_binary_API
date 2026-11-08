//! Adaptador IQ Option: nombres de mensaje y URLs del protocolo.

/// URLs base por host de configuración.
pub fn api_base_url(auth_host: &str) -> String {
    format!("https://{auth_host}/api")
}

/// Ruta de login (sobre la base).
pub const LOGIN_PATH: &str = "/v2/login";

pub fn ws_url(ws_host: &str) -> String {
    format!("wss://{ws_host}/echo/websocket")
}

/// Nombres de mensaje del protocolo WS (verificados en vivo, doc 04).
pub mod msg {
    /// Autenticación del socket.
    pub const SSID: &str = "ssid";
    /// Perfil + balances (respuesta al ssid).
    pub const PROFILE: &str = "profile";
    /// Listado de balances.
    pub const BALANCES: &str = "balances";
    /// Hora del servidor (ms).
    pub const TIME_SYNC: &str = "timeSync";
    /// Catálogo moderno (v3).
    pub const GET_INITIALIZATION_DATA: &str = "get-initialization-data";
    pub const INITIALIZATION_DATA: &str = "initialization-data";
    /// Catálogo legacy (v1).
    pub const API_OPTION_INIT_ALL: &str = "api_option_init_all";
    pub const API_OPTION_INIT_ALL_RESULT: &str = "api_option_init_all_result";
    /// Histórico de velas (legacy v2 y moderno v2 de minivelas).
    pub const GET_CANDLES: &str = "get-candles";
    pub const QUOTES_HISTORY_GET_CANDLES: &str = "quotes-history.get-candles";
    pub const CANDLES: &str = "candles";
    /// Streams.
    pub const CANDLE_GENERATED: &str = "candle-generated";
    pub const CANDLES_GENERATED: &str = "candles-generated";
    pub const BALANCE_CHANGED: &str = "balance-changed";
    pub const TRADERS_MOOD_CHANGED: &str = "traders-mood-changed";
    pub const TRADERS_MOOD: &str = "traders-mood";
    pub const TOP_ASSETS_UPDATED: &str = "top-assets-updated";
    pub const PRICE_SPLITTER: &str = "price-splitter.client-price-generated";
    /// Mensaje ENTRANTE del canal de precios (el nombre del canal es el de arriba).
    pub const CLIENT_PRICE_GENERATED: &str = "client-price-generated";
    /// Órdenes y ciclo de vida.
    pub const OPEN_OPTION: &str = "binary-options.open-option";
    pub const PLACE_DIGITAL_OPTION: &str = "digital-options.place-digital-option";
    pub const DIGITAL_OPTION_PLACED: &str = "digital-option-placed";
    pub const BUY_COMPLETE: &str = "buyComplete";
    pub const OPTION: &str = "option";
    pub const OPTION_OPENED: &str = "option-opened";
    pub const OPTION_CLOSED: &str = "option-closed";
    pub const SOCKET_OPTION_OPENED: &str = "socket-option-opened";
    pub const SOCKET_OPTION_CLOSED: &str = "socket-option-closed";
    /// Canales sociales y de comisiones (P4/P5).
    pub const LIVE_DEAL: &str = "live-deal";
    pub const LIVE_DEAL_BINARY: &str = "live-deal-binary-option-placed";
    pub const COMMISSION_CHANGED: &str = "commission-changed";

    /// Cartera: cierre de posiciones (digital y demás).
    pub const POSITION_CHANGED: &str = "position-changed";
    /// Venta anticipada de opciones (buyback).
    pub const SELL_OPTIONS: &str = "sell-options";
    pub const SOLD_OPTIONS: &str = "sold-options";

    /// Cartera: historial de posiciones cerradas.
    pub const PORTFOLIO_GET_HISTORY: &str = "portfolio.get-history-positions";
    pub const HISTORY_POSITIONS: &str = "history-positions";

    /// Perfil fundamental (GraphQL).
    pub const GET_FINANCIAL_INFORMATION: &str = "get-financial-information";
    pub const FINANCIAL_INFORMATION: &str = "financial-information";

    /// Indicadores técnicos del bróker (canal existe; hoy rechaza todos los
    /// activos con "Active is not supported", verificado 2026-10-27).
    pub const TECHNICAL_INDICATORS: &str = "trading-signals.get-technical-indicators";
    pub const TECHNICAL_INDICATORS_RESPONSE: &str = "technical-indicators";
    /// Sobre de envío.
    pub const SEND_MESSAGE: &str = "sendMessage";
    pub const SUBSCRIBE_MESSAGE: &str = "subscribeMessage";
    /// Cambio de cuenta activa y consulta de balances.
    pub const API_PROFILE_CHANGEBALANCE: &str = "api_profile_changebalance";
    pub const GET_BALANCES: &str = "get-balances";
}

/// `option_type_id` del protocolo.
pub mod option_type_id {
    pub const BINARY: u32 = 1;
    pub const TURBO: u32 = 3;
    pub const BLITZ: u32 = 12;
}

/// `balance type` observado en `profile.balances`.
pub mod balance_type {
    pub const REAL: i64 = 1;
    pub const PRACTICE: i64 = 4;
}

/// Prefijo que el bróker añade a los nombres de activos (`front.EURUSD`).
pub const ASSET_NAME_PREFIX: &str = "front.";

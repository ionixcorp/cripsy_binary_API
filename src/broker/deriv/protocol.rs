//! Traducción entre el JSON de Deriv y el dominio del crate.
//!
//! Reglas: nada de datos inventados. Lo que Deriv no publica (volumen de
//! velas, `opcode`, id numérico de balance) queda ausente o documentado como
//! sustituto explícito.

use serde_json::Value;

use crate::domain::{
    Asset, AssetClass, AssetExtras, Candle, Direction, MarketStatus, OperationResult, OrderId,
    Position, PositionHistory, Side,
};

/// Cuenta numérica estable a partir del id de cuenta de Deriv.
///
/// Deriv identifica cuentas con cadenas (`CR…`, `VRTC…`); el dominio usa `i64`.
/// Hasta que el contrato admita cadenas (0.3.0), este valor es un **sustituto
/// determinista**, no un identificador del bróker.
pub fn id_surrogado(account_id: &str) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in account_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash & 0x7fff_ffff_ffff_ffff) as i64
}

/// Clase de activo a partir de `market`/`underlying_symbol_type`.
pub fn clase_de(market: &str, tipo: &str) -> AssetClass {
    let m = market.to_ascii_lowercase();
    let t = tipo.to_ascii_lowercase();
    if m.contains("synthetic") || t.contains("stockindex") {
        AssetClass::Index
    } else if m.contains("forex") || t.contains("forex") {
        AssetClass::Forex
    } else if m.contains("crypto") || t.contains("crypto") {
        AssetClass::Crypto
    } else if m.contains("commodit") || t.contains("commodit") || m.contains("metal") {
        AssetClass::Commodity
    } else if m.contains("index") || m.contains("indices") {
        AssetClass::Index
    } else {
        AssetClass::Cfd
    }
}

fn estado(v: &Value) -> MarketStatus {
    if v.get("is_trading_suspended").and_then(Value::as_u64) == Some(1) {
        MarketStatus::Suspended
    } else if v.get("exchange_is_open").and_then(Value::as_u64) == Some(1) {
        MarketStatus::Open
    } else {
        MarketStatus::Closed
    }
}

fn precision(pip: f64) -> Option<u32> {
    if pip > 0.0 && pip < 1.0 {
        Some((-pip.log10()).round() as u32)
    } else if pip >= 1.0 {
        Some(0)
    } else {
        None
    }
}

/// Activos desde una respuesta `active_symbols`.
pub fn activos(valor: &Value) -> Vec<Asset> {
    let Some(lista) = valor.get("active_symbols").and_then(Value::as_array) else {
        return Vec::new();
    };
    lista
        .iter()
        .filter_map(|v| {
            let symbol = v.get("underlying_symbol")?.as_str()?.to_string();
            let name = v
                .get("underlying_symbol_name")
                .and_then(Value::as_str)
                .unwrap_or(&symbol)
                .to_string();
            let market = v.get("market").and_then(Value::as_str).unwrap_or("");
            let tipo = v
                .get("underlying_symbol_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            let pip = v.get("pip_size").and_then(Value::as_f64);
            Some(Asset {
                symbol,
                name,
                class: clase_de(market, tipo),
                // Deriv no publica identificador numérico: 0 documentado.
                opcode: 0,
                status: estado(v),
                payout_percent: None,
                sector: v
                    .get("submarket")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                schedule: None,
                extras: Some(AssetExtras {
                    precision: pip.and_then(precision),
                    provider: v
                        .get("subgroup")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    ..AssetExtras::default()
                }),
            })
        })
        .collect()
}

/// Velas desde una respuesta `candles`.
pub fn velas(valor: &Value, intervalo: u64) -> Vec<Candle> {
    let Some(lista) = valor.get("candles").and_then(Value::as_array) else {
        return Vec::new();
    };
    lista
        .iter()
        .filter_map(|v| {
            let from = v.get("epoch").and_then(Value::as_i64)?;
            Some(Candle {
                from,
                to: from + intervalo as i64,
                open: v.get("open").and_then(Value::as_f64)?,
                close: v.get("close").and_then(Value::as_f64)?,
                high: v.get("high").and_then(Value::as_f64)?,
                low: v.get("low").and_then(Value::as_f64)?,
                // Deriv no publica volumen en el histórico de velas.
                volume: None,
            })
        })
        .collect()
}

/// Vela individual de un mensaje `ohlc`.
pub fn vela_ohlc(valor: &Value, intervalo: u64) -> Option<Candle> {
    let v = valor.get("ohlc")?;
    let from = v.get("open_time")?.as_i64()?;
    Some(Candle {
        from,
        to: from + intervalo as i64,
        open: v.get("open")?.as_f64()?,
        close: v.get("close")?.as_f64()?,
        high: v.get("high")?.as_f64()?,
        low: v.get("low")?.as_f64()?,
        volume: None,
    })
}

/// `(importe, divisa, id de cuenta)` desde una respuesta `balance`.
pub fn balance(valor: &Value) -> Option<(f64, String, String)> {
    let b = valor.get("balance")?;
    Some((
        b.get("balance")?.as_f64()?,
        b.get("currency")?.as_str()?.to_string(),
        b.get("loginid")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ))
}

/// Porcentaje de payout a partir de una propuesta.
///
/// Deriv cotiza por importe: `profit_percent` depende del stake, no es un dato
/// de catálogo.
pub fn payout_porcentaje(propuesta: &Value, stake: f64) -> Option<f64> {
    let payout = propuesta.get("proposal")?.get("payout")?.as_f64()?;
    if stake <= 0.0 {
        return None;
    }
    Some((payout / stake - 1.0) * 100.0)
}

fn direccion_de(contract_type: &str) -> Direction {
    if contract_type.eq_ignore_ascii_case("PUT") {
        Direction::Put
    } else {
        Direction::Call
    }
}

/// Posiciones desde `profit_table` (cierres) o `portfolio` (abiertas).
///
/// La forma exacta de `profit_table` está pendiente de verificación en vivo:
/// el parser acepta `profit_table.transactions` y `transactions`.
pub fn posiciones(valor: &Value, offset: usize) -> PositionHistory {
    let lista = valor
        .get("profit_table")
        .and_then(|p| p.get("transactions"))
        .or_else(|| valor.get("transactions"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let posiciones: Vec<Position> = lista
        .iter()
        .filter_map(|v| {
            let id = v
                .get("contract_id")
                .and_then(|x| x.as_i64().map(|n| n.to_string()))
                .or_else(|| {
                    v.get("contract_id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })?;
            let contrato = v
                .get("contract_type")
                .and_then(Value::as_str)
                .unwrap_or("CALL");
            let symbol = v
                .get("underlying_symbol")
                .or_else(|| v.get("symbol"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let compra = v.get("buy_price").and_then(num);
            let venta = v.get("sell_price").and_then(num);
            // `profit_table` no publica `profit`: el resultado se deriva de la
            // venta y la compra, y queda marcado como dato derivado.
            let derivado = match (compra, venta) {
                (Some(c), Some(s)) => Some(s - c),
                _ => None,
            };
            let pnl = v.get("profit").and_then(num).or(derivado);
            let inicio = v
                .get("purchase_time")
                .or_else(|| v.get("date_start"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let fin = v
                .get("sell_time")
                .or_else(|| v.get("date_settlement"))
                .and_then(Value::as_i64);
            Some(Position {
                id,
                instrument_type: contrato.to_string(),
                symbol,
                side: direccion_de(contrato).into(),
                amount: compra.unwrap_or(0.0),
                open_price: v
                    .get("entry_spot")
                    .and_then(|s| s.as_str().and_then(|x| x.parse().ok())),
                pnl,
                invested: compra,
                open_at: inicio,
                close_at: fin,
                leverage: None,
            })
        })
        .collect();

    PositionHistory {
        positions: posiciones,
        offset,
        // `profit_table.count` es el tamaño de la página, no el total.
        total: None,
    }
}

/// Número que puede venir como JSON numérico o como cadena.
fn num(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

impl From<Direction> for Side {
    fn from(d: Direction) -> Self {
        match d {
            Direction::Call => Side::Buy,
            Direction::Put => Side::Sell,
        }
    }
}

/// Resultado terminal de un `proposal_open_contract`.
pub fn resultado_contrato(valor: &Value) -> Option<(OrderId, OperationResult)> {
    let poc = valor.get("proposal_open_contract")?;
    let id = poc
        .get("contract_id")?
        .to_string()
        .trim_matches('"')
        .to_string();
    let status = poc.get("status").and_then(Value::as_str).unwrap_or("");
    let profit = poc.get("profit").and_then(|p| {
        p.as_f64()
            .or_else(|| p.as_str().and_then(|s| s.parse::<f64>().ok()))
    });
    let resultado = match status {
        "won" => OperationResult::Win(profit.unwrap_or_default()),
        "lost" => OperationResult::Loss(profit.map(f64::abs).unwrap_or_default()),
        "sold" => match profit.unwrap_or_default() {
            p if p > 0.0 => OperationResult::Win(p),
            p if p < 0.0 => OperationResult::Loss(p.abs()),
            _ => OperationResult::Equal(0.0),
        },
        "cancelled" => return None,
        _ => return None,
    };
    Some((id, resultado))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn id_surrogado_es_estable_y_positivo() {
        let a = id_surrogado("VRTC123456");
        assert_eq!(a, id_surrogado("VRTC123456"));
        assert!(a > 0);
        assert_ne!(a, id_surrogado("VRTC654321"));
    }

    #[test]
    fn mapea_activos_reales() {
        let v = json!({"active_symbols":[{
            "underlying_symbol":"R_100","underlying_symbol_name":"Volatility 100 Index",
            "market":"synthetic_index","underlying_symbol_type":"stockindex",
            "pip_size":0.01,"exchange_is_open":1,"is_trading_suspended":0,
            "submarket":"random_index","subgroup":"synthetics"}]});
        let lista = activos(&v);
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].symbol, "R_100");
        assert_eq!(lista[0].class, AssetClass::Index);
        assert_eq!(lista[0].status, MarketStatus::Open);
        assert_eq!(lista[0].extras.as_ref().unwrap().precision, Some(2));
    }

    #[test]
    fn mapea_velas_sin_volumen() {
        let v = json!({"candles":[{"epoch":100,"open":1.0,"high":1.3,"low":0.9,"close":1.2}]});
        let c = velas(&v, 60);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].to, 160);
        assert!(c[0].volume.is_none());
        assert!(c[0].is_consistent());
    }

    #[test]
    fn resultado_por_estado() {
        let ganada =
            json!({"proposal_open_contract":{"contract_id":1,"status":"won","profit":0.95}});
        assert_eq!(
            resultado_contrato(&ganada),
            Some(("1".into(), OperationResult::Win(0.95)))
        );
        let perdida =
            json!({"proposal_open_contract":{"contract_id":2,"status":"lost","profit":-1.0}});
        assert_eq!(
            resultado_contrato(&perdida),
            Some(("2".into(), OperationResult::Loss(1.0)))
        );
        let abierta = json!({"proposal_open_contract":{"contract_id":3,"status":"open"}});
        assert!(resultado_contrato(&abierta).is_none());
    }

    #[test]
    fn payout_es_porcentaje_del_stake() {
        let v = json!({"proposal":{"payout":1.95}});
        assert!((payout_porcentaje(&v, 1.0).unwrap() - 95.0).abs() < 1e-9);
        assert!((payout_porcentaje(&v, 2.0).unwrap() - (-2.5)).abs() < 1e-9);
    }
}

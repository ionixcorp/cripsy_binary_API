//! Mapeo dominio ↔ protocolo IQ Option.

use serde_json::Value;

use crate::broker::config::AccountSelection;
use crate::broker::session::Session;
use crate::domain::*;
use crate::transport::websocket::WsFrame;

use super::endpoints::ASSET_NAME_PREFIX;

/// Quita el prefijo `front.` de los nombres del bróker.
pub fn simbolo_desde_nombre(nombre: &str) -> String {
    nombre
        .strip_prefix(ASSET_NAME_PREFIX)
        .unwrap_or(nombre)
        .to_string()
}

/// Convierte `balance type` del bróker en `AccountType`.
pub fn tipo_de_cuenta(balance_type: i64) -> AccountType {
    match balance_type {
        1 => AccountType::Real,
        4 => AccountType::Practice,
        _ => AccountType::Real,
    }
}

/// Payout en porcentaje a partir de la comisión del catálogo.
pub fn payout_de_comision(comision: f64) -> f64 {
    (100.0 - comision).max(0.0)
}

/// Parsea los grupos `binary`/`turbo`/`blitz` del inicializador.
pub fn catalogo_desde_init(msg: &Value) -> Vec<Asset> {
    // `groups` mapea group_id -> nombre de sector ("front.forex").
    let grupos: std::collections::HashMap<String, String> = msg
        .get("groups")
        .and_then(|g| g.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let mut activos = Vec::new();
    for (grupo, clase) in [
        ("binary", AssetClass::Binary),
        ("turbo", AssetClass::Turbo),
        ("blitz", AssetClass::Blitz),
        ("digital", AssetClass::Digital),
    ] {
        let Some(actives) = msg
            .pointer(&format!("/{grupo}/actives"))
            .and_then(|a| a.as_object())
        else {
            continue;
        };
        for (id, a) in actives {
            let Some(opcode) = id.parse::<u32>().ok() else {
                continue;
            };
            let nombre = a.get("name").and_then(|n| n.as_str()).unwrap_or_default();
            if nombre.is_empty() {
                continue;
            }
            let comision = a
                .pointer("/option/profit/commission")
                .and_then(|c| c.as_f64());
            let payout_percent = comision.map(payout_de_comision);
            let habilitado = a.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false);
            let suspendido = a
                .get("is_suspended")
                .and_then(|e| e.as_bool())
                .unwrap_or(false);
            // Estado declarado por el bróker; el horario se evalúa a consulta con
            // `Asset::is_open_by_schedule` usando la hora del servidor.
            let status = if !habilitado || suspendido {
                MarketStatus::Closed
            } else {
                MarketStatus::Open
            };
            let schedule = a
                .get("schedule")
                .and_then(|s| s.as_array())
                .map(|ventanas| {
                    ventanas
                        .iter()
                        .filter_map(|v| {
                            let abierta = v.as_array()?;
                            Some(ScheduleWindow {
                                open: abierta.first()?.as_i64()?,
                                close: abierta.get(1)?.as_i64()?,
                            })
                        })
                        .collect::<Vec<_>>()
                });
            let expiration_times = a
                .pointer("/option/expiration_times")
                .and_then(|e| e.as_array())
                .map(|v| v.iter().filter_map(|s| s.as_u64()).collect::<Vec<_>>());
            let rollovers = a.get("rollovers").and_then(|r| r.as_array()).map(|v| {
                v.iter()
                    .filter_map(|r| {
                        Some(RolloverParams {
                            deadtime: r.get("deadtime")?.as_u64()? as u32,
                            limit: r.get("limit")?.as_u64()? as u32,
                            offset: r.get("offset")?.as_u64()? as u32,
                            offset_from_expiration: r.get("offset_from_expiration")?.as_u64()?
                                as u32,
                            expiration_size: r.get("expiration_size")?.as_u64()? as u32,
                        })
                    })
                    .collect::<Vec<_>>()
            });
            let min_amount = a
                .pointer("/minmax/min")
                .and_then(|m| m.as_f64())
                .or_else(|| a.get("minimal_bet").and_then(|m| m.as_f64()));
            let max_amount = a
                .pointer("/minmax/max")
                .and_then(|m| m.as_f64())
                .or_else(|| a.get("maximal_bet").and_then(|m| m.as_f64()));
            let extras = AssetExtras {
                expiration_times_secs: expiration_times,
                rollovers,
                min_amount,
                max_amount,
                provider: a
                    .get("provider")
                    .and_then(|p| p.as_str())
                    .map(str::to_string),
                exchange: a
                    .get("exchange")
                    .and_then(|e| e.as_str())
                    .map(str::to_string),
                start_time: a.get("start_time").and_then(|t| t.as_i64()),
                buyback_deadtime: a
                    .get("buyback_deadtime")
                    .and_then(|d| d.as_u64())
                    .map(|d| d as u32),
                deadtime: a.get("deadtime").and_then(|d| d.as_u64()).map(|d| d as u32),
                is_buyback: a.get("is_buyback").and_then(|b| b.as_bool()),
                top_traders_enabled: a.get("top_traders_enabled").and_then(|b| b.as_bool()),
                precision: a
                    .get("precision")
                    .and_then(|p| p.as_u64())
                    .map(|p| p as u32),
                minimal_bet: a.get("minimal_bet").and_then(|m| m.as_f64()),
                maximal_bet: a.get("maximal_bet").and_then(|m| m.as_f64()),
            };
            // Sector: `groups` mapea group_id -> nombre ("front.forex").
            // Sector: `groups` mapea group_id -> nombre ("front.forex").
            // El `group_id` puede venir como string o como número.
            let sector = a
                .get("group_id")
                .and_then(|g| {
                    g.as_str()
                        .map(str::to_string)
                        .or_else(|| g.as_i64().map(|i| i.to_string()))
                })
                .and_then(|gid| grupos.get(&gid).map(|s| s.as_str()))
                .map(simbolo_desde_nombre);
            let _ = comision; // la comisión se consulta vía el catálogo del bróker
            activos.push(Asset {
                symbol: simbolo_desde_nombre(nombre),
                name: nombre.to_string(),
                class: clase,
                opcode,
                status,
                payout_percent,
                sector,
                schedule,
                extras: Some(extras),
            });
        }
    }
    activos
}

/// Parsea una vela del histórico (`get-candles` / `quotes-history.get-candles`).
///
/// Campos: `from`,`to`,`open`,`close`,`max`,`min`,`volume`,`at` (ns).
pub fn vela_desde_json(v: &Value) -> Option<Candle> {
    Some(Candle {
        from: v.get("from")?.as_i64()?,
        to: v.get("to")?.as_i64()?,
        open: v.get("open")?.as_f64()?,
        close: v.get("close")?.as_f64()?,
        high: v.get("max").and_then(|m| m.as_f64())?,
        low: v.get("min").and_then(|m| m.as_f64())?,
        volume: v.get("volume").and_then(|x| x.as_f64()),
    })
}

/// `expired` de blitz: el bróker exige el valor ALINEADO al múltiplo de la
/// duración (verificado en vivo 2026-10-27; valores desalineados reciben
/// `active is suspended`).
pub fn expiracion_blitz_alineada(ahora: i64, duracion_segs: u64) -> i64 {
    let d = duracion_segs as i64;
    (ahora / d + 1) * d
}

/// Tiempo de expiración de binarias/turbo (réplica de `expiration.py`).///
/// Devuelve `(expirado, indice)`. Los primeros 5 candidatos son de minuto
/// (turbo); el índice decide `option_type_id`: <5 → 3 (turbo), ≥5 → 1 (binary).
pub fn tiempo_de_expiracion(ahora: i64, duracion_min: u64) -> (i64, usize) {
    let minuto_base = ahora - ahora % 60;
    let mut candidatos: Vec<i64> = Vec::new();

    // Turbo: siguientes 5 minutos hábiles.
    let mut t = if minuto_base + 60 - ahora > 30 {
        minuto_base + 60
    } else {
        minuto_base + 120
    };
    for _ in 0..5 {
        candidatos.push(t);
        t += 60;
    }

    // Binary: cuartos de hora con margen de 5 minutos.
    let mut exp = minuto_base;
    let mut idx = 0;
    while idx < 50 {
        if (exp / 60) % 15 == 0 && exp - ahora > 60 * 5 {
            candidatos.push(exp);
            idx += 1;
        }
        exp += 60;
    }

    let objetivo = (60 * duracion_min) as i64;
    candidatos
        .into_iter()
        .enumerate()
        .min_by_key(|(_, t)| (*t - ahora - objetivo).abs())
        .map(|(i, t)| (t, i))
        .unwrap_or((ahora + 60 * duracion_min as i64, 0))
}

/// Payout digital en vivo desde `price-splitter.client-price-generated`.
///
/// Payout digital en vivo desde `price-splitter.client-price-generated`.
///
/// `call` paga con `bid`; `put` paga con `ask`: `ProfitPercent = (100-precio)*100/precio`.
pub fn payout_price_splitter(msg: &Value, direction: Direction) -> Option<Payout> {
    let prices = msg.get("prices")?.as_array()?;
    // La entrada spot se marca con strike = SPT (el resto son strikes numéricos).
    let primero = prices
        .iter()
        .find(|p| p.get("strike").and_then(|s| s.as_str()) == Some("SPT"))
        .or_else(|| prices.first())?;
    let precio = match direction {
        Direction::Call => primero.pointer("/call/bid").and_then(|v| v.as_f64())?,
        Direction::Put => primero.pointer("/put/ask").and_then(|v| v.as_f64())?,
    };
    if precio <= 0.0 {
        return None;
    }
    Some(Payout {
        symbol: msg
            .get("asset_id")
            .and_then(|a| a.as_i64())
            .map(|a| a.to_string())
            .unwrap_or_default(),
        expiration_seconds: msg.get("period").and_then(|p| p.as_u64()).unwrap_or(0),
        profit_percent: (100.0 - precio) * 100.0 / precio,
        is_spot: true,
        strike: None,
        closed: false,
    })
}

/// `instrument_id` digital por dirección y periodo (segundos) del price-splitter.
pub fn instrumento_digital(msg: &Value, periodo_segs: u64, direction: Direction) -> Option<String> {
    if msg.get("period").and_then(|p| p.as_u64()) != Some(periodo_segs) {
        return None;
    }
    let prices = msg.get("prices")?.as_array()?;
    let primero = prices
        .iter()
        .find(|p| p.get("strike").and_then(|s| s.as_str()) == Some("SPT"))
        .or_else(|| prices.first())?;
    match direction {
        Direction::Call => primero
            .pointer("/call/symbol")
            .and_then(|s| s.as_str())
            .map(str::to_string),
        Direction::Put => primero
            .pointer("/put/symbol")
            .and_then(|s| s.as_str())
            .map(str::to_string),
    }
}

/// Construye una `Operation` desde el mensaje `option-opened`.
pub fn operacion_desde_apertura(msg: &Value) -> Option<Operation> {
    let id = msg.get("option_id")?.to_string();
    let direction = match msg.get("direction").and_then(|d| d.as_str())? {
        "call" => Direction::Call,
        "put" => Direction::Put,
        _ => return None,
    };
    let option_type = match msg.get("option_type").and_then(|t| t.as_str()) {
        Some("blitz") => OptionType::Blitz,
        Some(_) => OptionType::Binary,
        None => OptionType::Binary,
    };
    Some(Operation {
        id,
        symbol: msg
            .get("active")
            .and_then(|a| a.as_str())
            .unwrap_or_default()
            .to_string(),
        amount: msg.get("amount").and_then(|a| a.as_f64()).unwrap_or(0.0),
        direction,
        option_type,
        status: OperationStatus::Open,
        result: None,
        // El bróker envía el CRÉDITO TOTAL (p. ej. 183 = 100 + 83): normalizamos
        // al payout neto (verificado en vivo; observación del informe 2026-10-31).
        payout_percent: msg
            .get("profit_percent")
            .and_then(|p| p.as_f64())
            .map(|p| (p - 100.0).max(0.0)),
        opened_at: msg
            .get("open_time")
            .and_then(|t| t.as_i64())
            .unwrap_or_default(),
        closed_at: None,
    })
}

/// Ventana de expiración de minivela válida (`to` en el cierre de una vela).
pub fn cierre_de_vela_cerrada(ahora_segs: i64, size_segs: i64, margen_segs: i64) -> i64 {
    (ahora_segs / size_segs) * size_segs - margen_segs
}

/// Redondeo monetario a 6 decimales (evita ruido de punto flotante).
pub fn redondear_dinero(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// Resultado neto a partir del mensaje de cierre (`option-closed`).
///
/// `win_enrolled_amount`/`profit_amount` son el CRÉDITO TOTAL (stake + ganancia).
pub fn resultado_desde_cierre(msg: &Value) -> Option<OperationResult> {
    let resultado = msg.get("result").and_then(|r| r.as_str())?;
    let stake = redondear_dinero(msg.get("amount").and_then(|a| a.as_f64()).unwrap_or(0.0));
    let credito = redondear_dinero(
        msg.get("win_enrolled_amount")
            .or_else(|| msg.get("profit_amount"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    );
    match resultado {
        // Un `win` SIN crédito es un mensaje incompleto (verificado en Blitz:
        // `socket-option-closed` puede llegar sin `win_amount`): se descarta
        // para no pisar el resultado bueno (informe 2026-10-31).
        "win" | "won" => {
            if credito > 0.0 {
                Some(OperationResult::Win(redondear_dinero(credito - stake)))
            } else {
                None
            }
        }
        "loose" | "lose" | "lost" => Some(OperationResult::Loss(stake)),
        "equal" => Some(OperationResult::Equal(stake)),
        // Cierre manual (venta anticipada/buyback): el bróker marca
        // `result: "default"` y el crédito real viaja en profit_amount.
        "default" | "sold" => {
            let venta = msg
                .get("profit_amount")
                .and_then(|p| p.as_f64())
                .or_else(|| msg.get("win_enrolled_amount").and_then(|w| w.as_f64()))
                .unwrap_or(0.0);
            let neto = redondear_dinero(venta - stake);
            if neto > 0.0 {
                Some(OperationResult::Win(neto))
            } else if neto < 0.0 {
                Some(OperationResult::Loss(-neto))
            } else {
                Some(OperationResult::Equal(stake))
            }
        }
        _ => None,
    }
}

/// Parsea los balances del mensaje `profile` (`/balances[]`).
pub fn balances_desde_perfil(perfil: &Value) -> Vec<AccountBalance> {
    let mut salida = Vec::new();
    if let Some(lista) = perfil.get("balances").and_then(|b| b.as_array()) {
        for b in lista {
            if let (Some(id), Some(tipo), Some(monto)) = (
                b.get("id").and_then(|i| i.as_i64()),
                b.get("type").and_then(|t| t.as_i64()),
                b.get("amount").and_then(|a| a.as_f64()),
            ) {
                salida.push(AccountBalance {
                    id,
                    currency: b
                        .get("currency")
                        .and_then(|c| c.as_str())
                        .unwrap_or("USD")
                        .to_string(),
                    amount: monto,
                    account_type: tipo_de_cuenta(tipo),
                    hold_amount: b.get("hold_amount").and_then(|v| v.as_f64()),
                    bonus_amount: b.get("bonus_amount").and_then(|v| v.as_f64()),
                    enrolled_amount: b.get("enrolled_amount").and_then(|v| v.as_f64()),
                    has_deposits: b.get("has_deposits").and_then(|v| v.as_bool()),
                });
            }
        }
    }
    salida
}

/// Parsea una lista directa de balances (respuesta al mensaje `balances`).
pub fn balances_desde_lista(lista: &Value) -> Vec<AccountBalance> {
    let mut salida = Vec::new();
    if let Some(items) = lista.as_array() {
        for b in items {
            if let (Some(id), Some(tipo)) = (
                b.get("id").and_then(|i| i.as_i64()),
                b.get("type").and_then(|t| t.as_i64()),
            ) {
                salida.push(AccountBalance {
                    id,
                    currency: b
                        .get("currency")
                        .and_then(|c| c.as_str())
                        .unwrap_or("USD")
                        .to_string(),
                    amount: b.get("amount").and_then(|a| a.as_f64()).unwrap_or(0.0),
                    account_type: tipo_de_cuenta(tipo),
                    hold_amount: b.get("hold_amount").and_then(|v| v.as_f64()),
                    bonus_amount: b.get("bonus_amount").and_then(|v| v.as_f64()),
                    enrolled_amount: b.get("enrolled_amount").and_then(|v| v.as_f64()),
                    has_deposits: b.get("has_deposits").and_then(|v| v.as_bool()),
                });
            }
        }
    }
    salida
}

/// Construye la `Session` y los balances desde el perfil, eligiendo la cuenta.
///
/// Sin selección explícita se prefiere la demo (type 4), igual que la web.
pub fn sesion_desde_perfil(
    perfil: &Value,
    seleccion: &AccountSelection,
) -> Option<(Session, Vec<AccountBalance>)> {
    let balances = balances_desde_perfil(perfil);
    if balances.is_empty() {
        return None;
    }
    let elegido = match seleccion {
        AccountSelection::Practice => balances
            .iter()
            .find(|b| b.account_type == AccountType::Practice),
        AccountSelection::Real => balances
            .iter()
            .find(|b| b.account_type == AccountType::Real),
        AccountSelection::Default => balances
            .iter()
            .find(|b| b.account_type == AccountType::Practice)
            .or_else(|| balances.first()),
    }
    .cloned()?;
    Some((
        Session {
            broker_name: "iq_option".into(),
            connected_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or_default(),
            account_id: perfil
                .get("user_id")
                .and_then(|u| u.as_i64())
                .map(|u| u.to_string()),
            balance_id: Some(elegido.id),
            account_type: elegido.account_type,
            server_time_offset_ms: 0,
        },
        balances,
    ))
}

/// Parsea un `balance-changed` (mensaje `current_balance`).
pub fn balance_desde_cambio(msg: &Value) -> Option<AccountBalance> {
    let bal = msg.get("current_balance")?;
    Some(AccountBalance {
        id: bal.get("id").and_then(|i| i.as_i64())?,
        currency: bal
            .get("currency")
            .and_then(|c| c.as_str())
            .unwrap_or("USD")
            .to_string(),
        amount: bal.get("amount").and_then(|a| a.as_f64()).unwrap_or(0.0),
        account_type: tipo_de_cuenta(bal.get("type").and_then(|t| t.as_i64()).unwrap_or(1)),
        hold_amount: bal.get("hold_amount").and_then(|v| v.as_f64()),
        bonus_amount: bal.get("bonus_amount").and_then(|v| v.as_f64()),
        enrolled_amount: bal.get("enrolled_amount").and_then(|v| v.as_f64()),
        has_deposits: bal.get("has_deposits").and_then(|v| v.as_bool()),
    })
}

/// Vela viva de `candle-generated` (mensaje de tamaño único): campos en `msg`.
pub fn vela_viva_unica(msg: &Value) -> Option<(u32, Candle)> {
    let active_id = msg.get("active_id").and_then(|a| a.as_u64())? as u32;
    let size = msg.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
    let precio = msg.get("value").and_then(|v| v.as_f64());
    let mut candle = vela_desde_json_live(msg, precio)?;
    if candle.volume.is_none() {
        candle.volume = None;
    }
    candle.to = candle.from + size as i64;
    Some((active_id, candle))
}

/// Vela de un mensaje vivo donde `close` puede faltar (vela en curso):
/// se usa el precio actual del mensaje (`value`) como cierre.
pub fn vela_desde_json_live(vela: &Value, precio_actual: Option<f64>) -> Option<Candle> {
    Some(Candle {
        from: vela.get("from")?.as_i64()?,
        to: vela.get("to")?.as_i64()?,
        open: vela.get("open")?.as_f64()?,
        close: vela
            .get("close")
            .and_then(|c| c.as_f64())
            .or(precio_actual)?,
        high: vela.get("max")?.as_f64()?,
        low: vela.get("min")?.as_f64()?,
        volume: vela.get("volume").and_then(|x| x.as_f64()),
    })
}

/// Velas vivas de `candles-generated` (multi-tamaño): `msg.candles` por tamaño.
pub fn velas_vivas_multi(msg: &Value) -> Vec<(u64, Candle)> {
    let mut salida = Vec::new();
    let Some(actives) = msg.get("candles").and_then(|c| c.as_object()) else {
        return salida;
    };
    let precio = msg.get("value").and_then(|v| v.as_f64()).or_else(|| {
        let bid = msg.get("bid").and_then(|v| v.as_f64())?;
        let ask = msg.get("ask").and_then(|v| v.as_f64())?;
        Some((bid + ask) / 2.0)
    });
    for (size, vela) in actives {
        if let Some(candle) = vela_desde_json_live(vela, precio) {
            let size = size.parse::<u64>().unwrap_or(0);
            salida.push((size, candle));
        }
    }
    salida
}

/// Cierre de posición digital desde `position-changed` (`portfolio`).
///
/// El mensaje no trae `order_ids`: se correlaciona por `instrument_id` contra
/// las órdenes digitales abiertas (las guarda la sesión al comprar).
/// `close_reason = expired` trae `close_profit` = crédito TOTAL
/// (neto = close_profit - invest); `default` trae `pnl_realized` ya neto.
pub fn cierre_digital_desde_position(msg: &Value) -> Option<(String, OperationResult)> {
    if msg.get("instrument_type").and_then(|t| t.as_str()) != Some("digital-option") {
        return None;
    }
    let raw = msg.get("raw_event")?;
    if raw.get("status").and_then(|s| s.as_str()) != Some("closed") {
        return None;
    }
    let instrument_id = msg
        .get("instrument_id")
        .and_then(|i| i.as_str())?
        .to_string();
    let numero = |v: Option<&Value>| -> Option<f64> {
        v.and_then(|x| {
            x.as_f64()
                .or_else(|| x.as_str().and_then(|s| s.parse::<f64>().ok()))
        })
    };
    let reason = raw
        .get("close_reason")
        .and_then(|r| r.as_str())
        .unwrap_or("expired");
    let result = if reason == "expired" {
        let close = numero(raw.get("close_profit").or_else(|| msg.get("close_profit")))?;
        let invest = numero(
            raw.get("invest")
                .or_else(|| raw.get("buy_amount"))
                .or_else(|| msg.get("invest")),
        )?;
        let diff = redondear_dinero(close - invest);
        if diff > 0.0 {
            OperationResult::Win(diff)
        } else if diff == 0.0 {
            OperationResult::Equal(invest)
        } else {
            OperationResult::Loss(-diff)
        }
    } else {
        let pnl = numero(raw.get("pnl_realized").or_else(|| msg.get("pnl_realized")))?;
        if pnl >= 0.0 {
            OperationResult::Win(redondear_dinero(pnl))
        } else {
            OperationResult::Loss(redondear_dinero(-pnl))
        }
    };
    Some((instrument_id, result))
}
/// Convierte un frame del stream en un evento de dominio cuando aplica./// Resultado desde `socket-option-closed`: `amount` va en micro-unidades (1e6);
/// `sum` es el stake legible y `win_amount` el crédito total.
pub fn resultado_desde_socket_cerrado(msg: &Value) -> Option<OperationResult> {
    let win = msg.get("win").and_then(|w| w.as_str())?;
    let stake = redondear_dinero(
        msg.get("sum")
            .and_then(|s| s.as_f64())
            .or_else(|| {
                msg.get("amount")
                    .and_then(|a| a.as_f64())
                    .map(|a| a / 1_000_000.0)
            })
            .unwrap_or(0.0),
    );
    let credito = redondear_dinero(
        msg.get("win_amount")
            .and_then(|w| w.as_f64())
            .unwrap_or(0.0),
    );
    match win {
        // Igual que `resultado_desde_cierre`: `win` sin crédito = mensaje
        // incompleto; se descarta en lugar de inventar Win(0.0).
        "win" | "won" => {
            if credito > 0.0 {
                Some(OperationResult::Win(redondear_dinero(credito - stake)))
            } else {
                None
            }
        }
        "equal" => Some(OperationResult::Equal(stake)),
        _ => Some(OperationResult::Loss(stake)),
    }
}

/// `(asset_id, Payout)` desde `price-splitter.client-price-generated` (call).
pub fn payout_price_splitter_con_id(msg: &Value) -> Option<(u32, Payout)> {
    let asset_id = msg.get("asset_id").and_then(|a| a.as_u64())? as u32;
    let payout = payout_price_splitter(msg, Direction::Call)?;
    Some((asset_id, payout))
}

/// Tick de precio desde un mensaje de stream (`candles-generated` /
/// `candle-generated`): el precio spot viaja en `value`/`bid`/`ask`, con `at`
/// en NANOSEGUNDOS. Densidad medida en vivo: ~3 ticks/s por activo, sin límite
/// de caudal por conexión (20 activos = 61 msg/s).
pub fn tick_desde_frame(msg: &Value) -> Option<PriceTick> {
    let price = msg.get("value").and_then(|v| v.as_f64())?;
    let at_ns = msg.get("at").and_then(|a| a.as_i64()).unwrap_or(0);
    Some(PriceTick {
        price,
        bid: msg.get("bid").and_then(|b| b.as_f64()),
        ask: msg.get("ask").and_then(|a| a.as_f64()),
        at_ms: at_ns / 1_000_000,
    })
}

/// Interpreta la respuesta de `trading-signals.get-technical-indicators`.
///
/// Si el bróker soporta el activo devuelve Ok(indicadores) con el `indicators`
/// del mensaje; si no, Err con el mensaje textual del bróker
/// (observado: `Active is not supported: active id 'X'`, status 4150).
pub fn indicadores_desde_respuesta(
    msg: &Value,
    symbol: &str,
) -> Result<TechnicalIndicators, String> {
    match msg.get("indicators") {
        Some(indicadores) => Ok(TechnicalIndicators {
            symbol: symbol.to_string(),
            indicators: indicadores.clone(),
        }),
        None => Err(msg
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("el bróker no devolvió indicadores")
            .to_string()),
    }
}

/// Live-deal (operación de un tercero) desde el canal del mismo nombre.
/// Forma defensiva: el bróker no documenta el payload completo.
pub fn live_deal_desde_frame(simbolo: &str, msg: &Value) -> Option<LiveDeal> {
    let option_type = match msg.get("option_type").and_then(|t| t.as_str()) {
        Some("digital-option") => OptionType::Digital,
        _ => OptionType::Binary,
    };
    let direction = match msg.get("direction").and_then(|d| d.as_str()) {
        Some("put") => Direction::Put,
        _ => Direction::Call,
    };
    Some(LiveDeal {
        symbol: simbolo.to_string(),
        option_type,
        direction,
        amount: msg.get("amount").and_then(|a| a.as_f64()).unwrap_or(0.0),
        created_at: msg
            .get("created_at")
            .and_then(|c| c.as_i64())
            .unwrap_or_default(),
        user_id: msg
            .get("user_id")
            .and_then(|u| u.as_i64())
            .map(|u| u.to_string()),
    })
}

/// Comisión cambiada desde el canal `commission-changed`.
pub fn comision_desde_frame(simbolo: &str, msg: &Value) -> Option<CommissionChange> {
    Some(CommissionChange {
        symbol: simbolo.to_string(),
        instrument_type: msg
            .get("instrument_type")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string(),
        commission_value: msg.pointer("/commission/value").and_then(|c| c.as_f64()),
    })
}

/// Query GraphQL del perfil de activo (verificada en vivo 2026-10-29). El
/// esquema del bróker ya no acepta los fragmentos Stock/CryptoCurrency sobre
/// `FinInfo`; esta versión mínima funciona para todo tipo de activo.
pub const QUERY_ASSET_PROFILE: &str = "query GetAssetProfileInfo($activeId:ActiveID!, $locale: LocaleName){\n active(id: $activeId) {\n id\n name(source: TradeRoom, locale: $locale)\n ticker\n charts { dtd { change } m1 { change } y1 { change } ytd { change } }\n }\n }";

/// Parsea la respuesta de `get-financial-information` (GraphQL).
pub fn perfil_desde_financial(simbolo: &str, msg: &Value) -> Result<AssetProfile, String> {
    let activo = msg
        .pointer("/data/active")
        .ok_or("sin data.active (ver errors del broker)")?;
    let numero = |v: Option<&Value>| v.and_then(|x| x.as_f64());
    Ok(AssetProfile {
        symbol: simbolo.to_string(),
        broker_name: activo
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_string(),
        ticker: activo
            .get("ticker")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string(),
        change_dtd: numero(activo.pointer("/charts/dtd/change")),
        change_m1: numero(activo.pointer("/charts/m1/change")),
        change_y1: numero(activo.pointer("/charts/y1/change")),
        change_ytd: numero(activo.pointer("/charts/ytd/change")),
    })
}

/// Historial de posiciones desde `portfolio.get-history-positions`
/// (`msg.positions[]`): posiciones cerradas con PnL realizado.
pub fn historial_desde_positions(msg: &Value, offset: usize) -> Option<PositionHistory> {
    let lista = msg.get("positions")?.as_array()?;
    let numero = |v: Option<&Value>| v.and_then(|x| x.as_f64());
    let posiciones: Vec<Position> = lista
        .iter()
        .map(|p| Position {
            id: p
                .get("external_id")
                .or_else(|| p.get("id"))
                .and_then(|i| i.as_i64())
                .map(|i| i.to_string())
                .unwrap_or_default(),
            instrument_type: p
                .get("instrument_type")
                .and_then(|t| t.as_str())
                .unwrap_or("digital-option")
                .to_string(),
            symbol: p
                .get("active_name")
                .and_then(|n| n.as_str())
                .map(simbolo_desde_nombre)
                .unwrap_or_default(),
            side: match p.get("direction").and_then(|d| d.as_str()) {
                Some("put") => Side::Sell,
                _ => Side::Buy,
            },
            amount: p
                .get("invest")
                .and_then(|a| a.as_f64())
                .or_else(|| numero(p.get("buy_amount")))
                .unwrap_or(0.0),
            open_price: p.get("open_quote").and_then(|q| q.as_f64()),
            pnl: p
                .get("close_profit")
                .and_then(|c| c.as_f64())
                .map(|c| c - p.get("invest").and_then(|i| i.as_f64()).unwrap_or(0.0)),
            invested: p.get("invest").and_then(|i| i.as_f64()),
            open_at: p
                .get("open_time")
                .and_then(|t| t.as_i64())
                .unwrap_or_default(),
            close_at: p.get("close_time").and_then(|t| t.as_i64()),
            leverage: None,
        })
        .collect();
    let total = msg
        .get("total")
        .and_then(|t| t.as_u64())
        .map(|t| t as usize);
    Some(PositionHistory {
        positions: posiciones,
        offset,
        total,
    })
}

/// Convierte un frame del stream en un evento de dominio cuando aplica.
pub fn evento_desde_frame(frame: &WsFrame) -> Option<Event> {
    match frame.name.as_str() {
        super::endpoints::msg::TRADERS_MOOD_CHANGED => {
            let symbol = frame
                .msg
                .get("asset_id")
                .and_then(|a| a.as_i64())
                .map(|id| id.to_string())?;
            let call_percent = frame.msg.get("value").and_then(|v| v.as_f64())? * 100.0;
            Some(Event::TradersMoodChanged {
                symbol,
                call_percent,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn quita_prefijo_front() {
        assert_eq!(simbolo_desde_nombre("front.EURUSD"), "EURUSD");
        assert_eq!(simbolo_desde_nombre("EURUSD"), "EURUSD");
        assert_eq!(simbolo_desde_nombre("front.ETHUSD-OTC"), "ETHUSD-OTC");
    }

    #[test]
    fn payout_desde_comision() {
        assert_eq!(payout_de_comision(18.0), 82.0);
        assert_eq!(payout_de_comision(14.0), 86.0);
    }

    #[test]
    fn tipo_de_cuenta_desde_type() {
        assert_eq!(tipo_de_cuenta(4), AccountType::Practice);
        assert_eq!(tipo_de_cuenta(1), AccountType::Real);
    }

    #[test]
    fn catalogo_parsea_tres_grupos() {
        let init = json!({
            "binary": { "actives": {
                "76": { "name": "front.EURUSD-OTC", "enabled": true, "option": { "profit": { "commission": 12 } },
                        "schedule": [[100, 200]], "minmax": {"min": 1, "max": 5000} } } },
            "turbo": { "actives": {
                "1": { "name": "front.XAUUSD", "enabled": true, "option": { "profit": { "commission": 30 } } } } },
            "blitz": { "actives": {
                "2250": { "name": "front.USDZAR-OTC", "enabled": true,
                          "option": { "profit": { "commission": 13 }, "expiration_times": [30, 60] },
                          "rollovers": [{"deadtime": 7, "limit": 5, "offset": 0, "offset_from_expiration": 30, "expiration_size": 30}],
                          "minmax": {"min": 1, "max": 20000} } } }
        });
        let activos = catalogo_desde_init(&init);
        assert_eq!(activos.len(), 3);
        let eur = activos.iter().find(|a| a.symbol == "EURUSD-OTC").unwrap();
        assert_eq!(eur.class, AssetClass::Binary);
        assert_eq!(eur.opcode, 76);
        assert!(eur.schedule.is_some());
        let blitz = activos
            .iter()
            .find(|a| a.class == AssetClass::Blitz)
            .unwrap();
        let extras = blitz.extras.as_ref().unwrap();
        assert_eq!(extras.expiration_times_secs, Some(vec![30, 60]));
        assert!(extras.rollovers.is_some());
        assert_eq!(extras.max_amount, Some(20000.0));
    }

    #[test]
    fn catalogo_expone_sector_y_metadatos() {
        // Payload real observado: groups + campos por activo que nadie leía.
        let init = json!({
            "groups": { "1": "front.forex", "16": "front.crypto currencies" },
            "turbo": { "actives": {
                "76": {
                    "name": "front.EURUSD-OTC", "enabled": true, "group_id": "1",
                    "provider": "OTC", "exchange": "front.exchange_eurusdotc",
                    "start_time": 1406941845, "buyback_deadtime": 10, "deadtime": 30,
                    "is_buyback": true, "top_traders_enabled": true, "precision": 6,
                    "minimal_bet": 1, "maximal_bet": 20000,
                    "option": { "profit": { "commission": 14 } }
                }
            } }
        });
        let activos = catalogo_desde_init(&init);
        let eur = activos.iter().find(|a| a.symbol == "EURUSD-OTC").unwrap();
        assert_eq!(eur.sector.as_deref(), Some("forex"));
        let extras = eur.extras.as_ref().expect("extras");
        assert_eq!(extras.provider.as_deref(), Some("OTC"));
        assert_eq!(extras.buyback_deadtime, Some(10));
        assert_eq!(extras.deadtime, Some(30));
        assert_eq!(extras.is_buyback, Some(true));
        assert_eq!(extras.start_time, Some(1406941845));
        assert_eq!(extras.precision, Some(6));
        assert_eq!(extras.maximal_bet, Some(20000.0));
    }

    #[test]
    fn historial_desde_positions_calcula_pnl_neto() {
        // Payload real observado (cuenta demo).
        let msg = json!({
            "positions": [{
                "active_id": 1380, "active_name": "front.USDZAR-OTC",
                "close_profit": 0, "close_reason": "expired",                 "close_time": 1793065260000i64,
                "open_time": 1793065200000i64,
                "external_id": 20328564504i64, "invest": 1, "instrument_type": "digital-option",
                "open_quote": 16.5, "pnl": -1
            }],
            "total": 1
        });
        let h = historial_desde_positions(&msg, 0).unwrap();
        assert_eq!(h.total, Some(1));
        let p = &h.positions[0];
        assert_eq!(p.id, "20328564504");
        assert_eq!(p.symbol, "USDZAR-OTC");
        assert_eq!(p.pnl, Some(-1.0));
        assert_eq!(p.invested, Some(1.0));
    }

    #[test]
    fn balances_con_campos_de_riesgo() {
        let perfil = json!({ "balances": [{
            "id": 5, "type": 4, "amount": 100.5, "currency": "USD",
            "hold_amount": 3, "bonus_amount": 1.5, "enrolled_amount": 2,
            "has_deposits": true
        }] });
        let b = balances_desde_perfil(&perfil);
        assert_eq!(b[0].hold_amount, Some(3.0));
        assert_eq!(b[0].bonus_amount, Some(1.5));
        assert_eq!(b[0].enrolled_amount, Some(2.0));
        assert_eq!(b[0].has_deposits, Some(true));
    }

    #[test]
    fn live_deal_desde_frame_basico() {
        // Forma defensiva: el bróker no documenta el payload completo.
        let msg = json!({
            "active_id": 76, "option_type": "binary-option",
            "direction": "put", "amount": 25, "created_at": 1793416554, "user_id": 991
        });
        let deal = live_deal_desde_frame("EURUSD-OTC", &msg).unwrap();
        assert_eq!(deal.symbol, "EURUSD-OTC");
        assert_eq!(deal.option_type, OptionType::Binary);
        assert_eq!(deal.direction, Direction::Put);
        assert_eq!(deal.amount, 25.0);
        assert_eq!(deal.user_id.as_deref(), Some("991"));
    }

    #[test]
    fn comision_desde_frame_extrae_valor() {
        // Forma verificada contra el handler de la referencia.
        let msg = json!({
            "instrument_type": "digital-option", "active_id": 76,
            "commission": { "value": 13 }
        });
        let c = comision_desde_frame("EURUSD-OTC", &msg).unwrap();
        assert_eq!(c.symbol, "EURUSD-OTC");
        assert_eq!(c.instrument_type, "digital-option");
        assert_eq!(c.commission_value, Some(13.0));
    }

    #[test]
    fn sector_ausente_es_none() {
        let init = json!({
            "groups": { "1": "front.forex" },
            "turbo": { "actives": { "76": { "name": "front.X", "enabled": true, "group_id": "99" } } }
        });
        let activos = catalogo_desde_init(&init);
        assert!(activos[0].sector.is_none());
    }

    #[test]
    fn vela_desde_json_parsea() {
        let v = json!({"from": 100, "to": 160, "open": 1.1, "close": 1.2, "max": 1.3, "min": 1.05, "volume": 7, "at": "1792927902000000000"});
        let c = vela_desde_json(&v).unwrap();
        assert_eq!(c.high, 1.3);
        assert_eq!(c.low, 1.05);
        assert_eq!(c.volume, Some(7.0));
        assert!(vela_desde_json(&json!({"from": 1})).is_none());
    }

    #[test]
    fn cierre_de_vela_cerrada_redondea() {
        // 1792927907 → suelo(1792927907/60)*60 = 1792927860; -90 → 1792927770
        assert_eq!(cierre_de_vela_cerrada(1792927907, 60, 90), 1792927770);
    }

    #[test]
    fn expiracion_blitz_alineada_redondea() {
        // 1793064502 con 30s → 1793064510 (múltiplo de 30 siguiente).
        assert_eq!(expiracion_blitz_alineada(1793064502, 30), 1793064510);
        // Ya alineado → salta al siguiente múltiplo.
        assert_eq!(expiracion_blitz_alineada(1793064510, 30), 1793064540);
        // 900s alinea a múltiplos de 900.
        assert_eq!(expiracion_blitz_alineada(1793064502, 900), 1793064600);
    }

    #[test]
    fn tiempo_de_expiracion_redondea() {
        // 999_999_960 es múltiplo de 60; ahora = segundo 07 de ese minuto.
        // Margen >30 s → primer candidato turbo = minuto + 60 = 1_000_000_020.
        let (exp, idx) = tiempo_de_expiracion(999_999_967, 1);
        assert_eq!(exp, 1_000_000_020);
        assert!(idx < 5, "1 minuto cae en el bloque turbo");
        // 30 minutos busca cuarto de hora: índice >= 5.
        let (exp30, idx30) = tiempo_de_expiracion(999_999_967, 30);
        assert!(idx30 >= 5, "30 minuto cae en el bloque binary");
        assert!(exp30 > 999_999_967);
        // Segundo 55 del minuto: margen <= 30 → primer candidato = minuto + 120.
        let (exp55, _) = tiempo_de_expiracion(1_000_000_015, 1);
        assert_eq!(exp55, 1_000_000_080);
    }

    #[test]
    fn indicadores_desde_respuesta_ok_y_error() {
        // Respuesta con indicadores (forma libre del bróker).
        let ok = json!({"indicators": [{"name": "SMA", "values": [1.0, 2.0]}]});
        let r = indicadores_desde_respuesta(&ok, "EURUSD").unwrap();
        assert_eq!(r.symbol, "EURUSD");
        assert_eq!(r.indicators, json!([{"name": "SMA", "values": [1.0, 2.0]}]));

        // Respuesta real de rechazo (verificado en vivo 2026-10-27).
        let err = json!({"message": "Active is not supported: active id '1380'"});
        let e = indicadores_desde_respuesta(&err, "USDZAR-OTC").unwrap_err();
        assert!(
            e.contains("not supported"),
            "mensaje del bróker preservado: {e}"
        );
    }

    #[test]
    fn win_sin_credito_no_es_cierre() {
        // Informe 2026-10-31: socket-option-closed de Blitz con win pero sin
        // win_amount: debe descartarse, no inventar Win(0.0).
        let opt = json!({ "result": "win", "amount": 1, "win_enrolled_amount": 0 });
        assert_eq!(resultado_desde_cierre(&opt), None);
        let sock = json!({ "win": "win", "amount": 1000000 });
        assert_eq!(resultado_desde_socket_cerrado(&sock), None);
    }

    #[test]
    fn win_con_credito_funciona() {
        let opt = json!({ "result": "win", "amount": 1, "win_enrolled_amount": 1.83 });
        assert_eq!(
            resultado_desde_cierre(&opt),
            Some(OperationResult::Win(0.83))
        );
        let sock = json!({ "win": "win", "amount": 1000000, "sum": 1, "win_amount": 1.83 });
        assert_eq!(
            resultado_desde_socket_cerrado(&sock),
            Some(OperationResult::Win(0.83))
        );
    }

    #[test]
    fn payout_percent_de_apertura_es_credito_total() {
        // Observación secundaria del informe: option-opened.profit_percent
        // llega como 183 (crédito total) para un payout de 83.
        let msg = json!({
            "option_id": 1, "direction": "call", "option_type": "blitz",
            "amount": 1, "profit_percent": 183, "open_time": 1793416554
        });
        let op = operacion_desde_apertura(&msg).unwrap();
        assert_eq!(op.payout_percent, Some(83.0));
    }

    #[test]
    fn tick_desde_frame_extrae_precio() {
        // Payload real: `at` en nanosegundos, precio en `value`.
        let msg = json!({
            "active_id": 1380, "at": 1793067441904000000i64, "value": 16.599655,
            "bid": 16.599515, "ask": 16.599895, "phase": "T"
        });
        let tick = tick_desde_frame(&msg).unwrap();
        assert_eq!(tick.price, 16.599655);
        assert_eq!(tick.at_ms, 1793067441904);
        assert_eq!(tick.ask, Some(16.599895));
        assert!(tick_desde_frame(&json!({"active_id": 1})).is_none());
    }

    #[test]
    fn velas_vivas_multi_sin_close_usa_value() {
        // Payload real observado: las entas en curso no traen `close`.
        let msg = json!({
            "active_id": 1380,
            "bid": 16.5781,
            "ask": 16.5783,
            "value": 16.5782,
            "phase": "T",
            "candles": {
                "1": {"from": 1792927915, "id": 193276907, "max": 16.578285, "min": 16.578095,
                      "open": 16.578285, "phase": "T", "to": 1792927916, "volume": 0},
                "60": {"from": 1792927860, "id": 1611212, "max": 16.582745, "min": 16.577935,
                       "open": 16.581445, "phase": "T", "to": 1792927920, "volume": 0}
            }
        });
        let velas = velas_vivas_multi(&msg);
        assert_eq!(velas.len(), 2);
        let (size_1, vela_1) = velas.iter().find(|(s, _)| *s == 1).unwrap();
        assert_eq!(*size_1, 1);
        assert_eq!(vela_1.close, 16.5782);
        assert!(vela_1.is_consistent());
    }

    #[test]
    fn perfil_desde_financial_graphql() {
        // Respuesta real verificada en vivo (query mínima sin fragmentos).
        let msg = json!({ "data": { "active": {
            "id": 1, "name": "EUR/USD", "ticker": "EURUSD",
            "charts": {
                "dtd": { "change": -0.1598 }, "m1": { "change": -1.9 },
                "y1": { "change": -3.97 }, "ytd": { "change": -4.97 }
            }
        } } });
        let p = perfil_desde_financial("EURUSD", &msg).unwrap();
        assert_eq!(p.broker_name, "EUR/USD");
        assert_eq!(p.ticker, "EURUSD");
        assert_eq!(p.change_dtd, Some(-0.1598));
        assert_eq!(p.change_ytd, Some(-4.97));
    }

    #[test]
    fn perfil_sin_data_es_error() {
        let msg = json!({ "data": null, "errors": [{"message": "x"}] });
        assert!(perfil_desde_financial("EURUSD", &msg).is_err());
    }

    #[test]
    fn cierre_por_venta_anticipada() {
        // Venta anticipada: `result: "default"` con el crédito en profit_amount.
        let msg = json!({
            "result": "default", "amount": 1,
            "profit_amount": 0.83, "win_enrolled_amount": 0.83
        });
        assert_eq!(
            resultado_desde_cierre(&msg),
            Some(OperationResult::Loss(0.17))
        );
        // Venta con ganancia.
        let msg2 = json!({ "result": "default", "amount": 1, "profit_amount": 1.2 });
        assert_eq!(
            resultado_desde_cierre(&msg2),
            Some(OperationResult::Win(0.2))
        );
    }

    #[test]
    fn resultado_neto_desde_cierre() {
        // Evidencia real: crédito 1.85, stake 1 → neto 0.85
        let msg = json!({"result": "win", "amount": 1, "win_enrolled_amount": 1.85});
        let r = resultado_desde_cierre(&msg).unwrap();
        assert_eq!(r, OperationResult::Win(0.85));
        let msg2 = json!({"result": "loose", "amount": 1});
        assert_eq!(
            resultado_desde_cierre(&msg2),
            Some(OperationResult::Loss(1.0))
        );
        let msg3 = json!({"result": "equal", "amount": 1});
        assert_eq!(
            resultado_desde_cierre(&msg3),
            Some(OperationResult::Equal(1.0))
        );
    }
}

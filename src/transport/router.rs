//! Router de correlación `request_id` → respuesta.
//!
//! El protocolo de los brókeres correlaciona respuestas por `request_id`.
//! Este router reemplaza el patrón de espera por polling: cada solicitud se
//! registra con un receptor; cuando llega el mensaje con ese `request_id`,
//! la respuesta se entrega al receptor. No hay estado global: si nadie espera
//! el id, el mensaje se descarta con traza.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;
use tokio::sync::oneshot;

/// Correlaciona identificadores de solicitud con sus receptores.
pub struct RequestRouter {
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    contador: Mutex<u64>,
}

impl Default for RequestRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestRouter {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            contador: Mutex::new(0),
        }
    }

    /// Registra una solicitud y devuelve `(request_id, receptor)`.
    ///
    /// El `request_id` es único dentro del router. Si el receptor se descarta
    /// antes de la respuesta, el mensaje se ignora en [`Self::deliver`].
    pub fn register(&self, prefijo: &str) -> (String, oneshot::Receiver<Value>) {
        let (tx, rx) = oneshot::channel();
        let mut contador = self.contador.lock().expect("router no envenenado");
        *contador += 1;
        let id = format!("{prefijo}-{}-{}", uuid_v4_simple(), *contador);
        let mut pending = self.pending.lock().expect("router no envenenado");
        pending.insert(id.clone(), tx);
        (id, rx)
    }

    /// Entrega `msg` al receptor pendiente con ese `request_id`.
    ///
    /// Devuelve `true` si había un receptor vivo. Ids desconocidos o tardíos
    /// devuelven `false` (nunca entra en pánico).
    pub fn deliver(&self, request_id: Option<&str>, msg: Value) -> bool {
        let id = match request_id {
            Some(i) if !i.is_empty() => i,
            _ => return false,
        };
        let tx = {
            let mut pending = self.pending.lock().expect("router no envenenado");
            pending.remove(id)
        };
        match tx {
            Some(tx) => tx.send(msg).is_ok(),
            None => false,
        }
    }

    /// Número de solicitudes esperando respuesta.
    pub fn pending_count(&self) -> usize {
        self.pending.lock().expect("router no envenenado").len()
    }
}

/// Identificador aleatorio corto sin dependencia externa.
fn uuid_v4_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{nanos:08x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn entrega_por_request_id() {
        let r = RequestRouter::new();
        let (id, mut rx) = r.register("test");
        assert_eq!(r.pending_count(), 1);
        assert!(r.deliver(Some(&id), json!({"ok": true})));
        assert_eq!(r.pending_count(), 0);
        let recibido = rx.try_recv().expect("mensaje pendiente");
        assert_eq!(recibido, json!({"ok": true}));
    }

    #[test]
    fn id_desconocido_se_descarta() {
        let r = RequestRouter::new();
        assert!(!r.deliver(Some("no-existe"), json!(null)));
        assert!(!r.deliver(None, json!(null)));
        assert!(!r.deliver(Some(""), json!(null)));
    }

    #[test]
    fn receptor_desperdiciado_se_ignora() {
        let r = RequestRouter::new();
        let (id, rx) = r.register("test");
        drop(rx);
        assert!(!r.deliver(Some(&id), json!(null)));
        assert_eq!(r.pending_count(), 0);
    }

    #[test]
    fn ids_son_unicos() {
        let r = RequestRouter::new();
        let (a, _rx_a) = r.register("x");
        let (b, _rx_b) = r.register("x");
        assert_ne!(a, b);
        assert_eq!(r.pending_count(), 2);
    }
}

//! Cuentas y balances.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccountType {
    Practice,
    Real,
    Tournament,
}

/// Balance de una cuenta concreta del bróker.
///
/// Además del monto disponible, expone los campos de riesgo que el bróker
/// publica y que la API de referencia no capturaba: saldo retenido, bonos,
/// matrículas y si la cuenta ha depositado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountBalance {
    /// `user_balance_id`: obligatorio para enviar órdenes.
    pub id: i64,
    pub currency: String,
    pub amount: f64,
    pub account_type: AccountType,
    /// Saldo retenido (operaciones abiertas).
    pub hold_amount: Option<f64>,
    /// Bonos acreditados.
    pub bonus_amount: Option<f64>,
    /// Saldo acumulado en el programa de la plataforma.
    pub enrolled_amount: Option<f64>,
    /// ¿La cuenta ha realizado depósitos?
    pub has_deposits: Option<bool>,
}

/// Balance de la cuenta activa de la sesión.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Balance {
    pub amount: f64,
    pub currency: String,
    pub account_type: AccountType,
    pub balance_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_es_clone_y_eq() {
        let b = Balance {
            amount: 10.0,
            currency: "USD".into(),
            account_type: AccountType::Practice,
            balance_id: 42,
        };
        let c = b.clone();
        assert_eq!(b, c);
        assert_eq!(b.balance_id, 42);
    }
}

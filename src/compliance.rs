//! Cumplimiento de consumo: garantía, descargo y advertencias de uso.
//!
//! Una librería no puede detectar quién la consume, pero sí puede **avisar**
//! cuando el uso se aparta del flujo documentado y dejar explícito el contrato
//! (garantía para el flujo documentado, ausencia de responsabilidad para el
//! resto). Ver la sección «Garantía y descargo» de `README.md`.

use crate::broker::config::{AccountSelection, IqOptionConfig, TransportConfig};

/// Versión de la API (del propio crate).
pub const API_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Dónde está la documentación de usuario.
pub const DOCS_INDEX: &str = "docs/site/es/index.html";

/// Garantía y descargo, en texto listo para mostrar al consumidor.
pub const GARANTIA: &str = "cripsy_binary_API garantiza el comportamiento \
documentado (docs/) cuando se sigue el flujo documentado: \
dependencia como librería, trait Broker canónico, canales documentados y compuerta \
verde. Cualquier otro uso queda fuera de la garantía y sus consecuencias no son \
responsabilidad de este equipo.";

/// Informe de cumplimiento para un consumo concreto.
#[derive(Debug, Clone, PartialEq)]
pub struct ComplianceReport {
    /// Versión de la librería que se está usando.
    pub api_version: &'static str,
    /// Features activas de compilación.
    pub features: Vec<&'static str>,
    /// Advertencias de uso apartado del flujo documentado (vacío = conforme).
    pub advertencias: Vec<String>,
}

impl ComplianceReport {
    /// ¿El consumo analizado no tiene advertencias?
    pub fn conforme(&self) -> bool {
        self.advertencias.is_empty()
    }
}

/// Features activas detectadas en tiempo de compilación.
pub fn features_activas() -> Vec<&'static str> {
    let mut f = Vec::new();
    if cfg!(feature = "async") {
        f.push("async");
    }
    if cfg!(feature = "blocking") {
        f.push("blocking");
    }
    f
}

/// Revisa una configuración y devuelve el informe de cumplimiento.
///
/// No bloquea nada: solo advierte. Las advertencias cubren los casos que la
/// experiencia contra el bróker real demostrado como problemáticos.
pub fn revisar_config(cfg: &IqOptionConfig) -> ComplianceReport {
    let mut advertencias = Vec::new();

    if cfg.timeout_secs == 0 {
        advertencias.push(
            "timeout_secs = 0: toda espera de respuesta quedará sin plazo; \
             usa un valor > 0 (el bróker puede no responder)"
                .into(),
        );
    }
    if !cfg.transport.verify_tls {
        advertencias.push(
            "verify_tls = false: la conexión acepta certificados inválidos; \
             solo para pruebas documentadas, nunca en producción"
                .into(),
        );
    }
    if cfg.transport.request_timeout_secs > cfg.timeout_secs {
        advertencias.push(
            "request_timeout_secs > timeout_secs: el transporte esperaría más \
             que el plazo de la solicitud"
                .into(),
        );
    }
    if cfg.account == AccountSelection::Real {
        advertencias.push(
            "cuenta Real seleccionada al conectar: para pruebas usa \
             AccountSelection::Practice (demo); las operaciones reales requieren \
             autorización explícita"
                .into(),
        );
    }
    if cfg.email.is_empty() || cfg.password.is_empty() {
        advertencias.push(
            "credenciales vacías: el login fallará; cárgalas desde .env \
             (variables de entorno), nunca en el código"
                .into(),
        );
    }
    for host in [&cfg.auth_host, &cfg.api_host, &cfg.ws_host] {
        if host.is_empty() {
            advertencias.push("host vacío en la configuración".into());
        }
    }

    ComplianceReport {
        api_version: API_VERSION,
        features: features_activas(),
        advertencias,
    }
}

/// Revisa una configuración de transporte suelta (sin datos de cuenta).
pub fn revisar_transporte(cfg: &TransportConfig) -> ComplianceReport {
    let mut advertencias = Vec::new();
    if !cfg.verify_tls {
        advertencias.push("verify_tls = false: solo para pruebas documentadas".into());
    }
    if cfg.max_retries > 20 {
        advertencias.push(
            "max_retries muy alto: con backoff máximo, una reconexión puede \
             tardar minutos; valores razonables: 3-10"
                .into(),
        );
    }
    ComplianceReport {
        api_version: API_VERSION,
        features: features_activas(),
        advertencias,
    }
}

/// Mensaje listo para mostrar al consumidor (garantía + descargo).
pub fn mensaje_consumidor() -> String {
    format!(
        "cripsy_binary_API {API_VERSION}\n\
         Documentación: {DOCS_INDEX}\n\
         Reglas de uso: README.md\n\
         {GARANTIA}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_conforme_sin_advertencias() {
        let cfg = IqOptionConfig {
            email: "u@ejemplo.com".into(),
            password: "secreto".into(),
            account: AccountSelection::Practice,
            ..IqOptionConfig::default()
        };
        let reporte = revisar_config(&cfg);
        assert!(
            reporte.conforme(),
            "no debía haber advertencias: {:?}",
            reporte.advertencias
        );
        assert_eq!(reporte.api_version, API_VERSION);
    }

    #[test]
    fn config_real_y_sin_tls_advierte() {
        let transporte = TransportConfig {
            verify_tls: false,
            ..TransportConfig::default()
        };
        let cfg = IqOptionConfig {
            email: "u@ejemplo.com".into(),
            password: "secreto".into(),
            account: AccountSelection::Real,
            transport: transporte,
            ..IqOptionConfig::default()
        };
        let reporte = revisar_config(&cfg);
        assert!(!reporte.conforme());
        assert!(reporte
            .advertencias
            .iter()
            .any(|a| a.contains("verify_tls")));
        assert!(reporte.advertencias.iter().any(|a| a.contains("Real")));
    }

    #[test]
    fn credenciales_vacias_advierten() {
        let cfg = IqOptionConfig {
            email: String::new(),
            password: String::new(),
            ..IqOptionConfig::default()
        };
        let reporte = revisar_config(&cfg);
        assert!(!reporte.conforme());
        assert!(reporte
            .advertencias
            .iter()
            .any(|a| a.contains("credenciales vacías")));
    }

    #[test]
    fn mensaje_incluye_garantia() {
        let m = mensaje_consumidor();
        assert!(m.contains("garantiza"));
        assert!(m.contains("README.md"));
        assert!(m.contains(API_VERSION));
    }
}

#[cfg(test)]
mod tests_ejemplo_documentado {
    use crate::broker::IqOptionConfig;
    // Réplica del ejemplo del README («Garantía y descargo»): el mismo código
    // que copiaría un consumidor externo (con el nombre del crate).
    #[test]
    fn ejemplo_de_consumidores_compila() {
        // Un consumidor externo escribe `cripsy_binary_api::`; dentro del crate
        // el equivalente es `crate::` (mismos ítems, mismo contrato).
        use crate::{mensaje_consumidor, revisar_config};
        let _texto = mensaje_consumidor();
        let cfg = IqOptionConfig::default();
        let reporte = revisar_config(&cfg);
        for aviso in &reporte.advertencias {
            let _ = aviso;
        }
        assert!(!reporte.conforme());
        assert!(!reporte.api_version.is_empty());
    }
}

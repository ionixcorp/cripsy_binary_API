# Python

Binding nativo del núcleo Rust: la misma sesión, catálogo, velas, ticks y
órdenes rise/fall, invocados desde Python.

## Viabilidad y decisión

`pip install` no instala crates de Rust: el crate de crates.io es código fuente
para Cargo. Para Python hace falta un **módulo de extensión nativo**. La vía
estándar es PyO3 + maturin, que compila el núcleo Rust como `cdylib`, produce
wheels por plataforma y los publica en PyPI.

La integración se hizo como **paquete independiente** (`python/`), con su propio
`Cargo.toml` y `pyproject.toml`. El crate publicado en crates.io **no cambia ni
gana dependencias**: `cargo add cripsy-binary-api` sigue igual.

## Estado de publicación

| Canal | Estado |
|---|---|
| crates.io (`cripsy-binary-api` 0.3.1) | Publicado y verificado |
| GitHub Release `v0.3.1` | Wheels Linux x64/arm64, Windows x64/arm64, macOS Intel/arm64 y sdist |
| PyPI | Publicación de 0.3.1 en curso |

El wheel es `abi3` para CPython 3.10 o superior. Mientras PyPI no tenga el
paquete, las dos formas reales de instalar son el wheel del release y la
instalación desde el repositorio.

## Instalación

### Wheels del release

```powershell
# Windows x64
pip install https://github.com/ionixcorp/cripsy_binary_API/releases/download/v0.3.1/cripsy_binary_api-0.3.1-cp310-abi3-win_amd64.whl
```

```bash
# Linux x86_64
pip install https://github.com/ionixcorp/cripsy_binary_API/releases/download/v0.3.1/cripsy_binary_api-0.3.1-cp310-abi3-manylinux2014_x86_64.manylinux_2_17_x86_64.whl

# macOS arm64
pip install https://github.com/ionixcorp/cripsy_binary_API/releases/download/v0.3.1/cripsy_binary_api-0.3.1-cp310-abi3-macosx_11_0_arm64.whl
```

### Desde el repositorio (cualquier plataforma con Rust 1.86+)

```powershell
pip install "cripsy-binary-api @ git+https://github.com/ionixcorp/cripsy_binary_API.git@v0.3.1#subdirectory=python"
```

pip instala maturin como backend de build y compila el módulo. Requiere Rust y
un compilador C instalados.

### Cuando el paquete esté en PyPI

```powershell
pip install cripsy-binary-api
```

## Uso

### Deriv en modo público (sin credenciales)

```python
from cripsy_binary_api import Client

cliente = Client.connect_deriv()
try:
    print(len(cliente.assets()), "activos")
    velas = cliente.candles("R_100", interval=60, count=5)
    print(velas[-1]["close"])
    print(cliente.payout("R_100", 60)["profit_percent"])
    print(len(cliente.ticks("R_100", seconds=5)))
finally:
    cliente.close()
```

### Deriv con cuenta demo

```python
import os
from cripsy_binary_api import Client

cliente = Client.connect_deriv(
    token=os.environ["DERIV_TOKEN"],
    app_id=os.environ["DERIV_APPID"],
    account_id=os.environ.get("DERIV_ACCOUNT_ID"),
)
try:
    print(cliente.session())
    print(cliente.balance())
finally:
    cliente.close()
```

### IQ Option

```python
import os
from cripsy_binary_api import Client

cliente = Client.connect_iqoption(
    os.environ["IQOPTION_EMAIL"],
    os.environ["IQOPTION_PASSWORD"],
    practice=True,
)
try:
    print(cliente.balance())
finally:
    cliente.close()
```

## Superficie

| Método | Devuelve | Notas |
|---|---|---|
| `Client.connect_deriv(...)` | `Client` | sin token: público |
| `Client.connect_iqoption(...)` | `Client` | `practice=True` por defecto |
| `name()` | str | `iq_option` o `deriv` |
| `is_connected()` | bool | estado del socket |
| `assets()` | lista de dicts | catálogo completo |
| `candles(symbol, interval, count)` | lista de dicts | `interval` en segundos |
| `asset_status(symbol)` | str | `Open` / `Closed` / `Suspended` |
| `payout(symbol, seconds)` | dict | payout del catálogo |
| `session()` / `balance()` | dict | requieren cuenta |
| `history(limit, offset, symbol)` | dict | posiciones cerradas |
| `ticks(symbol, seconds)` | lista de dicts | stream nativo, sin polling |
| `buy(symbol, amount, direction, minutes)` | str | `call` / `put`; demo |
| `buy_digital(symbol, amount, direction, minutes, strike)` | str | solo IQ Option |
| `buy_blitz(symbol, amount, direction, seconds, profit_percent)` | str | solo IQ Option |
| `indicators(symbol)` | dict | solo IQ Option |
| `sell(order_id)` | `None` | venta anticipada |
| `result(order_id)` | `Win`/`Loss`/`Equal`/`Pending` | lee caché |
| `wait_result(order_id, timeout_secs)` | igual | espera el evento |
| `close()` | `None` | cierra el WebSocket |

Los dicts conservan los nombres del dominio Rust (`symbol`, `close`,
`profit_percent`, `account_type`, ...). No se renombran ni se recalculan.

## Límites

- **Superficie sincrónica.** Cada llamada bloquea el hilo de Python; el GIL se
  libera durante la espera. Para concurrencia, usa un `Client` por hilo.
- **Digital, blitz e indicadores solo en IQ Option.** El binding los expone y
  Deriv los rechaza con `CripsyError`; no se simulan sobre rise/fall.
- **Sin `asyncio`.** No hay `await` en Python todavía; el núcleo async se usa
  internamente.
- **Una sesión por `Client`.** `close()` es obligatorio para liberar el socket.
- **Errores.** Todos los fallos del bróker llegan como `CripsyError` con el
  mensaje original; los argumentos inválidos lanzan `ValueError`.

## Compilar el wheel en local

```powershell
python -m pip install "maturin>=1.14,<2"
maturin build --release --manifest-path python/Cargo.toml --out python/dist
python -m pip install --force-reinstall python/dist/*.whl
python python/smoke_deriv.py
```

El workflow `.github/workflows/wheels.yml` construye wheels para Linux, Windows
y macOS, más el sdist, y los adjunta al release al empujar una etiqueta `v*`.
La publicación en PyPI está preparada pero desactivada hasta que exista el
secreto `PYPI_API_TOKEN`.

# cripsy-binary-api (Python)

Bindings nativos del núcleo Rust [`cripsy-binary-api`](https://crates.io/crates/cripsy-binary-api)
para IQ Option y Deriv. Wheel `abi3` para CPython 3.10 o superior.

```python
from cripsy_binary_api import Client

cliente = Client.connect_deriv()          # modo público, sin credenciales
print(len(cliente.assets()), "activos")
print(cliente.candles("R_100", interval=60, count=5)[-1]["close"])
cliente.close()
```

La documentación completa, límites y credenciales están en
[`docs/python/01_python.es.md`](https://github.com/ionixcorp/cripsy_binary_API/blob/master/docs/python/01_python.es.md).

Licencia MIT. Proyecto independiente: no afiliado a IQ Option ni a Deriv.

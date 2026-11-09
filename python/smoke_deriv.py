"""Prueba de humo contra Deriv en modo público (sin credenciales, sin órdenes)."""

from cripsy_binary_api import Client, CripsyError, __version__

print("version:", __version__)

cliente = Client.connect_deriv()
try:
    activos = cliente.assets()
    print("activos:", len(activos))
    print("primero:", activos[0]["symbol"], activos[0]["class"])

    velas = cliente.candles("R_100", interval=60, count=5)
    print("velas:", len(velas), "ultimo cierre:", velas[-1]["close"])

    payout = cliente.payout("R_100", 60)
    print("payout:", payout["profit_percent"], "cerrado:", payout["closed"])

    ticks = cliente.ticks("R_100", 5.0)
    print("ticks en 5 s:", len(ticks), ticks[:2])

    try:
        print("sesion:", cliente.session())
    except CripsyError as error:
        print("sesion no disponible en modo publico:", error)
finally:
    cliente.close()

print("ok")

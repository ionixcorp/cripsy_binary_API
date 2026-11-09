"""Prueba de humo de IQ Option (solo lectura, cuenta demo)."""

from pathlib import Path

from cripsy_binary_api import Client

valores = {}
for linea in Path(".env").read_text(encoding="utf-8-sig").splitlines():
    if "=" in linea and not linea.strip().startswith("#"):
        clave, valor = linea.split("=", 1)
        valores[clave.strip()] = valor.strip().strip('"').strip("'")

email = valores.get("IQOPTION_EMAIL")
password = valores.get("IQOPTION_PASSWORD")
if not email or not password:
    print("sin IQOPTION_EMAIL/IQOPTION_PASSWORD; prueba omitida")
    raise SystemExit(0)

cliente = Client.connect_iqoption(email, password, practice=True)
try:
    print("broker:", cliente.name())
    print("balance:", cliente.balance())
    print("activos:", len(cliente.assets()))
finally:
    cliente.close()

print("ok")

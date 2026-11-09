"""Pruebas offline del binding: no abren red."""

import unittest

import cripsy_binary_api as api


class BindingBasico(unittest.TestCase):
    def test_version(self):
        self.assertEqual(api.__version__, "0.3.1")

    def test_brokers(self):
        self.assertEqual(api.BROKERS, ["iq_option", "deriv"])

    def test_cliente_expone_constructores(self):
        self.assertTrue(hasattr(api.Client, "connect_deriv"))
        self.assertTrue(hasattr(api.Client, "connect_iqoption"))
        self.assertTrue(issubclass(api.CripsyError, Exception))

    def test_metodos_de_orden_presentes(self):
        for metodo in (
            "buy",
            "buy_digital",
            "buy_blitz",
            "indicators",
            "sell",
            "wait_result",
            "ticks",
        ):
            self.assertTrue(hasattr(api.Client, metodo))


if __name__ == "__main__":
    unittest.main()

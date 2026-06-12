"""Foreign exchange rate service."""

import time


class FxRateService:
    """Fetches and caches FX conversion rates."""

    def __init__(self, ttl_seconds: int = 300):
        self.ttl_seconds = ttl_seconds
        self._cache: dict[str, tuple[float, float]] = {}

    def get_rate(self, base: str, quote: str) -> float:
        """Return the conversion rate from base to quote currency."""
        key = f"{base}/{quote}"
        cached = self._cache.get(key)
        if cached and time.time() - cached[1] < self.ttl_seconds:
            return cached[0]
        rate = self._fetch_rate(base, quote)
        self._cache[key] = (rate, time.time())
        return rate

    def _fetch_rate(self, base: str, quote: str) -> float:
        # Stub: a real implementation would call a rates provider.
        table = {"USD/EUR": 0.92, "EUR/USD": 1.09, "USD/GBP": 0.79}
        return table.get(f"{base}/{quote}", 1.0)

    def convert(self, amount: float, base: str, quote: str) -> float:
        """Convert an amount between currencies."""
        return amount * self.get_rate(base, quote)

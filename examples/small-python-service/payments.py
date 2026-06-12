"""Payment validation and processing."""

from fx import FxRateService

SUPPORTED_CURRENCIES = {"USD", "EUR", "GBP"}
MAX_PAYMENT_USD = 50_000.0


class PaymentError(Exception):
    """Raised when a payment fails validation."""


class PaymentValidator:
    """Validates payments against business rules before processing."""

    def __init__(self, fx: FxRateService):
        self.fx = fx

    def validate(self, payment: dict) -> None:
        """Validate a payment dict; raises PaymentError on any violation."""
        amount = payment.get("amount")
        currency = payment.get("currency")
        if amount is None or amount <= 0:
            raise PaymentError("amount must be positive")
        if currency not in SUPPORTED_CURRENCIES:
            raise PaymentError(f"unsupported currency: {currency}")
        usd_amount = self.fx.convert(amount, currency, "USD")
        if usd_amount > MAX_PAYMENT_USD:
            raise PaymentError(f"amount exceeds limit: {usd_amount:.2f} USD")

    def validate_batch(self, payments: list[dict]) -> list[dict]:
        """Validate a batch; returns the valid subset."""
        valid = []
        for p in payments:
            try:
                self.validate(p)
                valid.append(p)
            except PaymentError:
                continue
        return valid


def process_payment(validator: PaymentValidator, payment: dict) -> str:
    """Validate then settle a payment, returning a settlement id."""
    validator.validate(payment)
    return f"settled-{payment['currency']}-{payment['amount']}"
# trailing comment

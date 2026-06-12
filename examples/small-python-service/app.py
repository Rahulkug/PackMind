"""Service entrypoint: wires auth, validation, and payment processing."""

from auth import require_auth
from fx import FxRateService
from payments import PaymentValidator, process_payment


def handle_payment_request(token: str, payment: dict) -> dict:
    """API handler: authenticate, validate, settle."""
    require_auth(token)
    fx = FxRateService()
    validator = PaymentValidator(fx)
    settlement_id = process_payment(validator, payment)
    return {"status": "ok", "settlement_id": settlement_id}


def main() -> None:
    fx = FxRateService()
    validator = PaymentValidator(fx)
    demo = {"amount": 100.0, "currency": "EUR"}
    validator.validate(demo)
    print(process_payment(validator, demo))


if __name__ == "__main__":
    main()

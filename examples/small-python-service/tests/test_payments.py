"""Tests for payment validation."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import pytest

from fx import FxRateService
from payments import PaymentError, PaymentValidator, process_payment


@pytest.fixture
def validator():
    return PaymentValidator(FxRateService())


def test_validate_accepts_valid_payment(validator):
    validator.validate({"amount": 100.0, "currency": "USD"})


def test_validate_rejects_negative_amount(validator):
    with pytest.raises(PaymentError):
        validator.validate({"amount": -5.0, "currency": "USD"})


def test_validate_rejects_unsupported_currency(validator):
    with pytest.raises(PaymentError):
        validator.validate({"amount": 10.0, "currency": "XYZ"})


def test_validate_fx_path_converts_to_usd(validator):
    # 60,000 EUR is over the USD limit after conversion
    with pytest.raises(PaymentError):
        validator.validate({"amount": 60_000.0, "currency": "EUR"})


def test_process_payment_returns_settlement_id(validator):
    sid = process_payment(validator, {"amount": 10.0, "currency": "GBP"})
    assert sid.startswith("settled-GBP")

# small-python-service

A tiny payment service used as the PackMind demo and test fixture.

## Architecture

`app.py` is the entrypoint. `handle_payment_request` authenticates the caller
via `require_auth`, then builds a `PaymentValidator` backed by `FxRateService`
and settles the payment with `process_payment`.

## Payment validation

`PaymentValidator` enforces business rules: positive amounts, supported
currencies, and a USD-converted limit computed through `FxRateService`.

## Try PackMind on it

```bash
cd examples/small-python-service
packmind init
packmind index .
packmind pack "Refactor PaymentValidator to use FxRateService" --budget 4000
packmind callers process_payment
packmind tests PaymentValidator
```

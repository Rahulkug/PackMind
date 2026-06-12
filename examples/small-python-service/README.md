# small-python-service

A tiny payment service used as the PrefixGraph demo and test fixture.

## Architecture

`app.py` is the entrypoint. `handle_payment_request` authenticates the caller
via `require_auth`, then builds a `PaymentValidator` backed by `FxRateService`
and settles the payment with `process_payment`.

## Payment validation

`PaymentValidator` enforces business rules: positive amounts, supported
currencies, and a USD-converted limit computed through `FxRateService`.

## Try PrefixGraph on it

```bash
cd examples/small-python-service
prefixgraph init
prefixgraph index .
prefixgraph pack "Refactor PaymentValidator to use FxRateService" --budget 4000
prefixgraph callers process_payment
prefixgraph tests PaymentValidator
```

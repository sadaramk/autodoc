# Fulfillment

_Container_ · [Book index](../README.md)

Ships placed orders and notifies customers

**5** Files · **91** Lines · **5** Modules · **2** Depends on · **1** Used by

## At a glance

|   |   |
|---|---|
| Role | Background worker |
| Technology | `Python` |
| Path | `fulfillment/` |
| Manifest | `fulfillment/pyproject.toml` |
| Compose service | `fulfillment` |
| Entry points | `if \_\_name\_\_ == "\_\_main\_\_"` guard  [`fulfillment/fulfillment/__main__.py:22-23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/__main__.py#L22-L23) |

## Components

![Fulfillment — components](../diagrams/components-fulfillment.svg)

_Components of Fulfillment: modules and the imports between them._ · [IR](../diagrams/components-fulfillment.ir.json)

## Modules

| Module | Path | Responsibility | Symbols | Evidence |
|---|---|---|---|---|
| **Main** _entry_ | `fulfillment/fulfillment/__main__.py` | Entry point for the fulfillment worker process. | 2 | [`fulfillment/fulfillment/__main__.py:22-23`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/__main__.py#L22-L23) |
| **Consumer** | `fulfillment/fulfillment/consumer.py` | Kafka consumer for order events. | 3 | [`fulfillment/fulfillment/consumer.py:11-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L11-L29) |
| **Notify** | `fulfillment/fulfillment/notify.py` | Customer notifications via SendGrid. | 1 | [`fulfillment/fulfillment/notify.py:9-19`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/notify.py#L9-L19) |
| **Shipping** | `fulfillment/fulfillment/shipping.py` | Shipment creation and order status updates. | 1 | [`fulfillment/fulfillment/shipping.py:9-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L9-L17) |
| **Fulfillment** | `fulfillment/fulfillment/__init__.py` | Fulfillment worker: turns placed orders into shipments. | 0 | [`fulfillment/fulfillment/__init__.py:1-3`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/__init__.py#L1-L3) |

## Depends on

| Target | Interaction | Basis | Evidence |
|---|---|---|---|
| **PostgreSQL** | writes orders | _observed in code_ | [`fulfillment/fulfillment/shipping.py:9-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L9-L17) [`fulfillment/fulfillment/shipping.py:6`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L6) |
| **SendGrid** | sends email | _observed in code_ | [`fulfillment/fulfillment/notify.py:5`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/notify.py#L5) |

## Used by

| Caller | Interaction | Basis | Evidence |
|---|---|---|---|
| **Kafka** | delivers order.placed | _observed in code_ | [`fulfillment/fulfillment/consumer.py:19-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L19-L29) [`docker-compose.yml:41`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/docker-compose.yml#L41) |

## Key symbols

| Symbol | Kind | Description | Evidence |
|---|---|---|---|
| `OrderConsumer` | class | Subscribes to `order.placed` and dispatches each order to a handler. | [`fulfillment/fulfillment/consumer.py:11-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L11-L29) |
| `handle_order` | function | Ship one order and email the customer. | [`fulfillment/fulfillment/__main__.py:10-13`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/__main__.py#L10-L13) |
| `main` | function | Consume `order.placed` forever. | [`fulfillment/fulfillment/__main__.py:16-19`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/__main__.py#L16-L19) |
| `send_shipped_email` | function | Email the customer that their order has shipped. | [`fulfillment/fulfillment/notify.py:9-19`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/notify.py#L9-L19) |
| `ship_order` | function | Create a shipment, mark the order shipped, and return the tracking number. | [`fulfillment/fulfillment/shipping.py:9-17`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/shipping.py#L9-L17) |
| `run` | method | Poll forever, invoking `handler` for every decoded order. | [`fulfillment/fulfillment/consumer.py:19-29`](https://github.com/sadaramk/autodoc/blob/e7b68ce276f2e4470e6daf110d740e81698c7f96/tests/fixtures/polyglot-shop/fulfillment/fulfillment/consumer.py#L19-L29) |


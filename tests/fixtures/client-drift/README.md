# client-drift

Four units around one published operation, each caller parsing the response its own way:

- `accounts` (Java/Spring) publishes `GET /accounts/{id}` → `AccountView` and `GET /accounts` → `List<AccountView>`.
- `billing` (Java) calls both: a Feign interface returning its own `AccountView` (extra `currency`) and a
  `RestTemplate.exchange` with `ParameterizedTypeReference<List<AccountSummary>>` (extra `tier`).
- `reporting` (Python) reads keys straight off `response.json()` (`overdraft`) and validates into a pydantic
  model (`AccountSnapshot`, extra `openedAt`).
- `sync` (Go) decodes into a struct with an extra `lastLogin` field.

Every extra field is response drift: the caller reads something the operation never declares.

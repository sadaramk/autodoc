# attribution

Three units that make a large JVM system look emptier than it is:

- `dao` puts every write in a generic base class (`JpaAbstractDao.save`). The base never names an
  entity; the concrete DAOs do, through the accessor they override and the type arguments they bind.
  `JpaAbstractAuditDao` has no subclass — nothing may be attributed to it.
- `orders` and `reporting` both declare a `customer` table in their own code, with different columns:
  two tables, not one.
- `orders` serves the same route twice, told apart by `params=`: two operations, not one.

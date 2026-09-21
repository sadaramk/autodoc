using Catalog.Data;
using Catalog.Models;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Mvc;

namespace Catalog.Controllers;

/// <summary>Place and amend orders.</summary>
[ApiController]
[Route("v1/orders")]
[Authorize(Policy = "orders:write")]
public class OrdersController : ControllerBase
{
    private readonly CatalogContext _db;

    public OrdersController(CatalogContext db)
    {
        _db = db;
    }

    /// <summary>Replace an order.</summary>
    [HttpPut("{reference}")]
    public ActionResult<Order> Replace(
        [FromRoute] string reference,
        [FromHeader(Name = "X-Idempotency-Key")] [Required] string idempotencyKey,
        [FromBody] Order order)
    {
        if (reference != order.Id.ToString())
        {
            throw new ArgumentException("reference does not match body");
        }
        return order;
    }

    /// <summary>Look an order up by its external reference.</summary>
    [HttpGet("by-reference")]
    public ActionResult<Order> ByReference([FromQuery(Name = "ref")] string reference)
    {
        var order = _db.Orders.FirstOrDefault(o => o.Id.ToString() == reference);
        if (order is null)
        {
            throw new KeyNotFoundException(reference);
        }
        return order;
    }
}

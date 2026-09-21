using Catalog.Data;
using Catalog.Models;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Mvc;
using Microsoft.EntityFrameworkCore;

namespace Catalog.Controllers;

/// <summary>Read and write the product catalog.</summary>
[ApiController]
[Route("api/[controller]")]
[Authorize]
public class ProductsController : ControllerBase
{
    private readonly CatalogContext _db;

    public ProductsController(CatalogContext db)
    {
        _db = db;
    }

    /// <summary>List products, newest first.</summary>
    [HttpGet]
    [AllowAnonymous]
    public async Task<ActionResult<List<Product>>> List([FromQuery] int limit = 20, [FromQuery] Category? category = null)
    {
        var q = _db.Products.AsQueryable();
        if (category is not null)
        {
            q = q.Where(p => p.Category == category);
        }
        return await q.OrderByDescending(p => p.Id).Take(limit).ToListAsync();
    }

    /// <summary>Fetch one product.</summary>
    [HttpGet("{id:int}")]
    [AllowAnonymous]
    public async Task<ActionResult<Product>> Get([FromRoute] int id)
    {
        var product = await _db.Products.FindAsync(id);
        if (product is null)
        {
            return NotFound();
        }
        return product;
    }

    /// <summary>Create a product.</summary>
    [HttpPost]
    [ProducesResponseType(StatusCodes.Status201Created)]
    public async Task<ActionResult<Product>> Create([FromBody] NewProduct body)
    {
        var product = new Product { Name = body.Name, Price = body.Price };
        _db.Products.Add(product);
        await _db.SaveChangesAsync();
        return CreatedAtAction(nameof(Get), new { id = product.Id }, product);
    }

    /// <summary>Remove a product.</summary>
    [HttpDelete("{id:int}")]
    [Authorize(Roles = "admin")]
    public async Task<IActionResult> Delete(int id)
    {
        var product = await _db.Products.FindAsync(id);
        if (product is null)
        {
            return NotFound();
        }
        _db.Products.Remove(product);
        await _db.SaveChangesAsync();
        return NoContent();
    }
}

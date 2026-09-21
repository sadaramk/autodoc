using Catalog.Models;
using Microsoft.EntityFrameworkCore;

namespace Catalog.Data;

/// <summary>EF Core context over the catalog schema.</summary>
public class CatalogContext : DbContext
{
    public CatalogContext(DbContextOptions<CatalogContext> options) : base(options) { }

    public DbSet<Product> Products => Set<Product>();

    public DbSet<Order> Orders => Set<Order>();

    protected override void OnModelCreating(ModelBuilder b)
    {
        b.Entity<Product>().ToTable("products");
        b.Entity<Order>().ToTable("orders");
    }
}

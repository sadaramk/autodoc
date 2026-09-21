using System.ComponentModel.DataAnnotations;

namespace Catalog.Models;

/// <summary>The body accepted when creating a product.</summary>
public record NewProduct
{
    [Required]
    [StringLength(120, MinimumLength = 2)]
    public string Name { get; init; } = string.Empty;

    [Range(0, 100000)]
    public decimal Price { get; init; }

    [EmailAddress]
    public string? SupplierEmail { get; init; }
}

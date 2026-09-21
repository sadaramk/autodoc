using System.ComponentModel.DataAnnotations;

namespace Catalog.Models;

/// <summary>A product on the shelf.</summary>
public class Product
{
    [Key]
    public int Id { get; set; }

    [Required]
    [MaxLength(120)]
    public string Name { get; set; } = string.Empty;

    [Range(0, 100000)]
    public decimal Price { get; set; }

    public Category Category { get; set; }
}

public enum Category
{
    Tools,
    Garden,
}

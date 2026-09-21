using System.ComponentModel.DataAnnotations;
using System.Text.Json.Serialization;

namespace Catalog.Models;

/// <summary>A placed order.</summary>
public class Order
{
    [Key]
    public int Id { get; set; }

    [JsonPropertyName("customer_email")]
    [Required]
    [EmailAddress]
    public string CustomerEmail { get; set; } = string.Empty;

    [RegularExpression("^[A-Z]{3}$")]
    public string Currency { get; set; } = "USD";

    public Product? Item { get; set; }
}

using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Routing;

namespace Acme.PublicApi;

/// What a caller sends to create a catalog item.
public class CreateCatalogItemRequest
{
    public string Name { get; set; } = string.Empty;
    public decimal Price { get; set; }
}

/// What the endpoint returns once the item exists.
public class CreateCatalogItemResponse
{
    public int Id { get; set; }
    public string Name { get; set; } = string.Empty;
}

public class CatalogItemListResponse
{
    public List<string> Names { get; set; } = new();
}

/// The shape eShopOnWeb uses: the contract is declared by the lambda's first
/// parameter and by `.Produces<T>()`, not by an attribute on a controller.
public class CatalogItemEndpoints
{
    public void AddRoutes(IEndpointRouteBuilder app)
    {
        app.MapPost("api/catalog-items",
            [Authorize(Roles = "ADMINISTRATORS")] async
            (CreateCatalogItemRequest request, IRepository<CatalogItem> itemRepository) =>
            {
                return await HandleAsync(request, itemRepository);
            })
            .Produces<CreateCatalogItemResponse>()
            .WithTags("CatalogItemEndpoints");

        app.MapGet("api/catalog-items", async (IRepository<CatalogItem> itemRepository) =>
            {
                return await ListAsync(itemRepository);
            })
            .Produces<CatalogItemListResponse>()
            .WithTags("CatalogItemEndpoints");

        // A failure status is not the success shape, so this must not become the
        // operation's response.
        app.MapDelete("api/catalog-items/{catalogItemId}", async (int catalogItemId) => Results.NoContent())
            .Produces<ProblemDetails>(StatusCodes.Status404NotFound)
            .WithTags("CatalogItemEndpoints");
    }
}

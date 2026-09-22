using Catalog.Data;
using Microsoft.EntityFrameworkCore;

var builder = WebApplication.CreateBuilder(args);

builder.Services.AddControllers();
builder.Services.AddDbContext<CatalogContext>(o =>
    o.UseNpgsql(builder.Configuration.GetConnectionString("Catalog")));

var app = builder.Build();

app.MapControllers();
app.MapGet("/healthz", () => Results.Ok("ok"));
// No leading slash: ASP.NET Core routes this relative to the app root, and
// real code writes it both ways — eShopOnWeb writes every endpoint this way.
app.MapGet("api/stock", () => Results.Ok(new { inStock = 0 }));

app.Run();

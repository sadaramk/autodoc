using Catalog.Data;
using Microsoft.EntityFrameworkCore;

var builder = WebApplication.CreateBuilder(args);

builder.Services.AddControllers();
builder.Services.AddDbContext<CatalogContext>(o =>
    o.UseNpgsql(builder.Configuration.GetConnectionString("Catalog")));

var app = builder.Build();

app.MapControllers();
app.MapGet("/healthz", () => Results.Ok("ok"));

app.Run();

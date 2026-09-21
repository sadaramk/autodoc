# aspnet

ASP.NET Core 8 attribute-routed Web API. Exercises `[ApiController]` + class `[Route("api/[controller]")]`
with the `[controller]` token, method `[HttpGet("{id}")]`, `[FromQuery]` / `[FromRoute]` / `[FromBody]`
binding, `Results.NotFound()` / `CreatedAtAction`, data annotations on a DTO, `[Authorize]` /
`[AllowAnonymous]`, an EF Core `DbContext` with `DbSet<T>`, minimal-API `MapGet` in `Program.cs`,
and a `/healthz` probe that must be excluded.

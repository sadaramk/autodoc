# csharp-minimal-api

ASP.NET Core minimal APIs as eShopOnWeb writes them: the request type is the
lambda's first non-injected parameter, the response is `.Produces<T>()`, and
route strings carry no leading slash. Measured in #48, every eShopOnWeb
operation came out `Opaque` because none of that was read.

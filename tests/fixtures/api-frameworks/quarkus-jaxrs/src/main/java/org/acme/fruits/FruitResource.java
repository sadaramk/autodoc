package org.acme.fruits;

import java.util.List;

import io.quarkus.security.Authenticated;
import jakarta.annotation.security.RolesAllowed;
import jakarta.validation.Valid;
import jakarta.validation.constraints.Max;
import jakarta.ws.rs.*;
import jakarta.ws.rs.core.MediaType;
import jakarta.ws.rs.core.Response;

/** Fruit catalogue. */
@Path("/fruits")
@Produces(MediaType.APPLICATION_JSON)
@Consumes(MediaType.APPLICATION_JSON)
public class FruitResource {

    @GET
    public List<Fruit> list(@QueryParam("season") Season season,
                            @QueryParam("limit") @DefaultValue("50") @Max(200) int limit) {
        return List.of();
    }

    @GET
    @Path("/{id}")
    public Fruit get(@PathParam("id") long id) {
        throw new NotFoundException("fruit not found");
    }

    @POST
    @RolesAllowed("admin")
    public Response create(@Valid NewFruit fruit) {
        if (fruit.season() == Season.WINTER) {
            throw new OutOfSeasonException("not in season");
        }
        return Response.status(Response.Status.CREATED).entity(fruit).build();
    }

    @DELETE
    @Path("/{id}")
    @Authenticated
    public void delete(@PathParam("id") long id) {
    }
}

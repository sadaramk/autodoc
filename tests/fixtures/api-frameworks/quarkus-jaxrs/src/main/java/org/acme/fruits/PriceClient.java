package org.acme.fruits;

import jakarta.ws.rs.GET;
import jakarta.ws.rs.Path;
import jakarta.ws.rs.PathParam;
import org.eclipse.microprofile.rest.client.inject.RegisterRestClient;

@RegisterRestClient(configKey = "prices")
@Path("/prices")
public interface PriceClient {
    @GET
    @Path("/{fruitId}")
    Price current(@PathParam("fruitId") long fruitId);

    record Price(long fruitId, long cents) {
    }
}

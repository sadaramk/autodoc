package org.acme.fruits;

import jakarta.ws.rs.GET;
import jakarta.ws.rs.Path;

@Path("/healthz")
public class HealthResource {
    @GET
    public String ok() {
        return "ok";
    }
}

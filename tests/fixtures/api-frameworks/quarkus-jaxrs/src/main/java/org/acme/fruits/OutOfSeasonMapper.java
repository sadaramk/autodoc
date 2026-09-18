package org.acme.fruits;

import jakarta.ws.rs.core.Response;
import jakarta.ws.rs.ext.ExceptionMapper;
import jakarta.ws.rs.ext.Provider;

@Provider
public class OutOfSeasonMapper implements ExceptionMapper<OutOfSeasonException> {
    @Override
    public Response toResponse(OutOfSeasonException e) {
        return Response.status(422).entity(e.getMessage()).build();
    }
}

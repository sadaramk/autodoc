package org.acme.devices;

import jakarta.inject.Inject;
import jakarta.transaction.Transactional;
import jakarta.ws.rs.*;
import java.util.List;

@Path("/devices")
public class DeviceResource {
    @Inject
    DeviceEventRepository deviceEvents;

    @GET
    public List<Device> list() {
        return Device.listAll();
    }

    @POST
    @Path("/{id}/activate")
    @Transactional
    public Device activate(@PathParam("id") Long id) {
        Device device = Device.findById(id);
        if (device.state != DeviceState.PROVISIONED) {
            throw new BadRequestException("not provisioned");
        }
        device.state = DeviceState.ACTIVE;
        DeviceEvent event = new DeviceEvent();
        event.device = device;
        deviceEvents.persist(event);
        return device;
    }

    @DELETE
    @Path("/{id}")
    @Transactional
    public void retire(@PathParam("id") Long id) {
        Device device = Device.findById(id);
        device.state = DeviceState.RETIRED;
        device.persist();
    }
}

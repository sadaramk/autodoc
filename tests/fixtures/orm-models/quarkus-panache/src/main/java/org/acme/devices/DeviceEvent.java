package org.acme.devices;

import io.quarkus.hibernate.orm.panache.PanacheEntityBase;
import jakarta.persistence.*;

@Entity
@Table(name = "device_events")
public class DeviceEvent extends PanacheEntityBase {
    @Id
    @GeneratedValue
    public Long eventId;

    @ManyToOne
    @JoinColumn(name = "device_id")
    public Device device;

    public String kind;
}

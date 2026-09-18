package org.acme.devices;

import io.quarkus.hibernate.orm.panache.PanacheEntity;
import jakarta.persistence.*;

@Entity
@Table(name = "devices")
public class Device extends PanacheEntity {
    @Column(unique = true, nullable = false)
    public String serial;

    @Enumerated(EnumType.STRING)
    public DeviceState state = DeviceState.PROVISIONED;
}

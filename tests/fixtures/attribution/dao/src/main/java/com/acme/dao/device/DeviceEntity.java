package com.acme.dao.device;

import com.acme.dao.BaseEntity;
import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Table;
import java.util.UUID;

@Entity
@Table(name = "device")
public class DeviceEntity extends BaseEntity<Device> {

    @Id
    private UUID id;

    @Column(name = "name", nullable = false)
    private String name;

    @Column(name = "label")
    private String label;

    @Override
    public Device toData() {
        return new Device(id, name, label);
    }
}

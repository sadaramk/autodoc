package com.acme.dao.device;

import com.acme.dao.JpaAbstractDao;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.stereotype.Component;

@Component
public class JpaDeviceDao extends JpaAbstractDao<DeviceEntity, Device> {

    @Autowired
    private DeviceRepository deviceRepository;

    @Override
    protected JpaRepository<DeviceEntity, UUID> getRepository() {
        return deviceRepository;
    }

    @Override
    protected DeviceEntity toEntity(Device device) {
        return new DeviceEntity();
    }
}

package com.acme.dao.asset;

import com.acme.dao.BaseEntity;
import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.Table;
import java.util.UUID;

@Entity
@Table(name = "asset")
public class AssetEntity extends BaseEntity<Asset> {

    @Id
    private UUID id;

    @Column(name = "name", nullable = false)
    private String name;

    @Override
    public Asset toData() {
        return new Asset(id, name);
    }
}

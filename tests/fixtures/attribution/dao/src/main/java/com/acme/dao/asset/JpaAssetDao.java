package com.acme.dao.asset;

import com.acme.dao.JpaAbstractDao;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.stereotype.Component;

@Component
public class JpaAssetDao extends JpaAbstractDao<AssetEntity, Asset> {

    @Autowired
    private AssetRepository assetRepository;

    @Override
    protected JpaRepository<AssetEntity, UUID> getRepository() {
        return assetRepository;
    }

    @Override
    protected AssetEntity toEntity(Asset asset) {
        return new AssetEntity();
    }
}

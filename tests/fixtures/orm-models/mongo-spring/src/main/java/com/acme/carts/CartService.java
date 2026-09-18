package com.acme.carts;

import org.springframework.data.mongodb.core.MongoTemplate;
import org.springframework.data.mongodb.core.query.Query;
import org.springframework.stereotype.Service;

@Service
public class CartService {
    private final CartRepository cartRepository;
    private final MongoTemplate mongoTemplate;

    public CartService(CartRepository cartRepository, MongoTemplate mongoTemplate) {
        this.cartRepository = cartRepository;
        this.mongoTemplate = mongoTemplate;
    }

    public Cart open(String sessionKey) {
        return cartRepository.findBySessionKey(sessionKey).orElseGet(() -> cartRepository.save(new Cart()));
    }

    public void purgeAbandoned(Query abandoned) {
        mongoTemplate.remove(abandoned, Cart.class);
    }

    public Voucher voucher(String code) {
        return mongoTemplate.findById(code, Voucher.class);
    }
}

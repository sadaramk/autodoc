package com.acme.carts;

import org.springframework.data.annotation.Id;
import org.springframework.data.mongodb.core.index.Indexed;
import org.springframework.data.mongodb.core.mapping.DBRef;
import org.springframework.data.mongodb.core.mapping.Document;
import org.springframework.data.mongodb.core.mapping.Field;
import java.util.List;

@Document(collection = "carts")
public class Cart {
    @Id
    private String id;

    @Field("customer_ref")
    private String customerId;

    @Indexed(unique = true)
    private String sessionKey;

    private List<CartLine> lines;

    @DBRef
    private Voucher voucher;
}

package com.acme.carts;

import org.springframework.data.annotation.Id;
import org.springframework.data.mongodb.core.mapping.Document;

@Document
public class Voucher {
    @Id
    private String code;

    private int percentOff;
}

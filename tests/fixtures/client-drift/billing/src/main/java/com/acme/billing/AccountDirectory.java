package com.acme.billing;

import java.util.List;
import org.springframework.core.ParameterizedTypeReference;
import org.springframework.http.HttpMethod;
import org.springframework.http.ResponseEntity;
import org.springframework.stereotype.Component;
import org.springframework.web.client.RestTemplate;

@Component
public class AccountDirectory {

    private final RestTemplate restTemplate = new RestTemplate();

    public List<AccountSummary> all() {
        ResponseEntity<List<AccountSummary>> response = restTemplate.exchange(
                "http://accounts/accounts",
                HttpMethod.GET,
                null,
                new ParameterizedTypeReference<List<AccountSummary>>() {
                });
        return response.getBody();
    }
}

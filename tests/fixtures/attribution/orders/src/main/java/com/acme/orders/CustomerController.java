package com.acme.orders;

import java.util.List;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class CustomerController {

    private final CustomerRepository customers;

    public CustomerController(CustomerRepository customers) {
        this.customers = customers;
    }

    /** Everyone, paged by the caller. */
    @GetMapping("/customers")
    public List<Customer> all() {
        return customers.findAll();
    }

    /** The same route, chosen when the request carries an email. */
    @GetMapping(value = "/customers", params = {"email"})
    public List<Customer> byEmail(@RequestParam("email") String email) {
        return customers.findAll();
    }

    /** And again, for a support lookup by account number. */
    @GetMapping(value = "/customers", params = "accountNumber")
    public List<Customer> byAccount(@RequestParam("accountNumber") String accountNumber) {
        return customers.findAll();
    }
}

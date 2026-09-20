package com.acme.orders.web;

import com.acme.orders.domain.OrderService;
import org.springframework.stereotype.Controller;
import org.springframework.ui.Model;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestMapping;

/** Server-rendered order pages: these return view names, not payloads. */
@Controller
@RequestMapping("/ui/orders")
public class OrderPagesController {

    private final OrderService orders;

    public OrderPagesController(OrderService orders) {
        this.orders = orders;
    }

    /** Renders the order list template. */
    @GetMapping
    public String list(Model model) {
        model.addAttribute("orders", orders.list(null, 50));
        return "orders/list";
    }

    /** Renders one order's detail template. */
    @GetMapping("/{id}")
    public String detail(@PathVariable Long id, Model model) {
        model.addAttribute("order", orders.find(id));
        return "orders/detail";
    }
}

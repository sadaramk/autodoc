package com.acme.shop.web

import com.acme.shop.domain.OrderService
import com.acme.shop.domain.OrderStatus
import jakarta.validation.Valid
import jakarta.validation.constraints.Email
import jakarta.validation.constraints.NotBlank
import jakarta.validation.constraints.Size
import org.springframework.http.HttpStatus
import org.springframework.http.ResponseEntity
import org.springframework.security.access.prepost.PreAuthorize
import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.PathVariable
import org.springframework.web.bind.annotation.PostMapping
import org.springframework.web.bind.annotation.RequestBody
import org.springframework.web.bind.annotation.RequestMapping
import org.springframework.web.bind.annotation.RequestParam
import org.springframework.web.bind.annotation.ResponseStatus
import org.springframework.web.bind.annotation.RestController
import org.springframework.web.server.ResponseStatusException

/** Orders the shop serves over HTTP. */
@RestController
@RequestMapping("/orders")
class OrderController(private val orders: OrderService) {

    /** Reads one order. */
    @GetMapping("/{id}")
    fun byId(@PathVariable id: Long): ResponseEntity<OrderView> {
        val order = orders.byId(id) ?: throw ResponseStatusException(HttpStatus.NOT_FOUND, "order not found")
        return ResponseEntity.ok(OrderView(order.id, order.customerName, order.status))
    }

    @GetMapping
    @PreAuthorize("hasRole('ADMIN')")
    fun list(
        @RequestParam(required = false) status: OrderStatus?,
        @RequestParam(defaultValue = "20") limit: Int,
    ): List<OrderView> = emptyList()

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    fun place(@Valid @RequestBody body: PlaceOrder): OrderView {
        val order = orders.place(body.customerName)
        return OrderView(order.id, order.customerName, order.status)
    }
}

/** Request body for placing an order. */
data class PlaceOrder(
    @field:NotBlank @field:Size(max = 80) val customerName: String,
    @field:Email val email: String? = null,
    val note: String? = null,
)

data class OrderView(val id: Long, val customerName: String, val status: OrderStatus)

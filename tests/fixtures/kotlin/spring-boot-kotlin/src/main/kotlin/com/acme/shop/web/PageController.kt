package com.acme.shop.web

import org.springframework.stereotype.Controller
import org.springframework.web.bind.annotation.GetMapping

/** Server-rendered pages: these return view names, not payloads. */
@Controller
class PageController {

    /** Renders the orders template; not an HTTP API operation. */
    @GetMapping("/orders/list")
    fun list(): String = "orders/list"
}

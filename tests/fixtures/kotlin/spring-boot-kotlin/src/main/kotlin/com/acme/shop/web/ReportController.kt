package com.acme.shop.web

import org.springframework.web.bind.annotation.PathVariable
import org.springframework.web.bind.annotation.RequestMapping
import org.springframework.web.bind.annotation.RequestMethod
import org.springframework.web.bind.annotation.RestController

/** Reports, mounted under a prefix that only configuration knows. */
@RestController
@RequestMapping("\${api.prefix}/reports")
class ReportController {

    /** Answers every verb, because no method is named. */
    @RequestMapping("/summary")
    fun summary(): String = "ok"

    /** One handler, two verbs and two paths: Spring registers all four. */
    @RequestMapping(value = ["/daily", "/nightly"], method = [RequestMethod.POST, RequestMethod.PUT])
    fun run(@PathVariable id: String): String = "ran"
}

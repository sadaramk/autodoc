import { Body, Controller, Delete, Get, HttpCode, NotFoundException, Param, Post, Query, UseGuards } from "@nestjs/common";
import { AuthGuard } from "@nestjs/passport";
import { Roles } from "../roles.decorator";
import { CreateOrderDto, OrderDto } from "./dto";

@Controller("orders")
@UseGuards(AuthGuard("jwt"))
export class OrdersController {
  /** Looks up one order. */
  @Get(":id")
  async findOne(@Param("id") id: string, @Query("expand") expand?: string): Promise<OrderDto> {
    throw new NotFoundException("order not found");
  }

  @Post()
  async create(@Body() dto: CreateOrderDto): Promise<OrderDto> {
    return { id: "1", sku: dto.sku, quantity: dto.quantity, status: "pending" };
  }

  @Delete(":id")
  @HttpCode(204)
  @Roles("admin")
  async remove(@Param("id") id: string): Promise<void> {}
}

import { Module } from "@nestjs/common";
import { OrdersController } from "./orders/orders.controller";

@Module({ controllers: [OrdersController] })
export class AppModule {}

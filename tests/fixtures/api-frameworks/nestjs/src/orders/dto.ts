import { IsIn, IsInt, IsOptional, IsString, Length, Min } from "class-validator";

/** Body of POST /api/orders. */
export class CreateOrderDto {
  @IsString()
  @Length(3, 12)
  sku: string;

  @IsInt()
  @Min(1)
  quantity: number;

  @IsOptional()
  @IsIn(["standard", "express"])
  shipping?: string;
}

/** An order as the API returns it. */
export class OrderDto {
  id: string;
  sku: string;
  quantity: number;
  status: "pending" | "shipped";
}

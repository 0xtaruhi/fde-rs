`timescale 1ns/1ps

module tb;
  reg clk = 0;
  reg rst;
  reg i0;
  reg i1;
  reg i2;
  wire y0;
  wire y1;
  wire y2;
  wire y3;
  integer payload;

  `DUT dut(
      .clk(clk), .rst(rst),
      .i0(i0), .i1(i1), .i2(i2),
      .y0(y0), .y1(y1), .y2(y2), .y3(y3)
  );

  always #1 clk = ~clk;

  initial begin
    for (payload = 0; payload < 8; payload = payload + 1) begin
      {i2, i1, i0} = payload[2:0];
      rst = 1;
      repeat (4) @(posedge clk);
      @(negedge clk);
      $display("outputs=0x%0h", {y3, y2, y1, y0});

      rst = 0;
      repeat (32) @(posedge clk);
      @(negedge clk);
      $display("outputs=0x%0h", {y3, y2, y1, y0});
    end
    $finish;
  end
endmodule

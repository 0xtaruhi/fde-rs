`timescale 1ns/1ps

module tb;
  reg i0;
  reg i1;
  reg i2;
  reg i3;
  wire y0;
  wire y1;
  wire y2;
  wire y3;
  integer pattern;

  `DUT dut(
      .i0(i0), .i1(i1), .i2(i2), .i3(i3),
      .y0(y0), .y1(y1), .y2(y2), .y3(y3)
  );

  initial begin
    for (pattern = 0; pattern < 16; pattern = pattern + 1) begin
      {i3, i2, i1, i0} = pattern[3:0];
      #1;
      $display("outputs=0x%0h", {y3, y2, y1, y0});
    end
    $finish;
  end
endmodule

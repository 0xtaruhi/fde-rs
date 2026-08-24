module comb_boolean(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  assign y0 = i0 & i1;
  assign y1 = i2 | i3;
  assign y2 = i0 ^ i2;
  assign y3 = ~(i1 ^ i3);
endmodule

module comb_mux(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  assign y0 = i2 ? i1 : i0;
  assign y1 = i3 ? i2 : i1;
  assign y2 = i0 ? i3 : i2;
  assign y3 = i1 ? i0 : i3;
endmodule

module comb_decoder(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  wire enable = i2 ^ i3;
  assign y0 = enable & ~i1 & ~i0;
  assign y1 = enable & ~i1 &  i0;
  assign y2 = enable &  i1 & ~i0;
  assign y3 = enable &  i1 &  i0;
endmodule

module comb_compare(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  wire [1:0] lhs = {i3, i2};
  wire [1:0] rhs = {i1, i0};
  assign y0 = lhs == rhs;
  assign y1 = lhs < rhs;
  assign y2 = lhs > rhs;
  assign y3 = lhs != rhs;
endmodule

module comb_adder(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  wire [2:0] sum = {1'b0, i3, i2} + {1'b0, i1, i0};
  assign {y2, y1, y0} = sum;
  assign y3 = i0 ^ i1 ^ i2 ^ i3;
endmodule

module comb_subtractor(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  wire [1:0] lhs = {i3, i2};
  wire [1:0] rhs = {i1, i0};
  wire [2:0] difference = {1'b0, lhs} - {1'b0, rhs};
  assign {y2, y1, y0} = difference;
  assign y3 = lhs >= rhs;
endmodule

module comb_rotate(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  assign y0 = i1;
  assign y1 = i2;
  assign y2 = i3;
  assign y3 = i0;
endmodule

module comb_priority(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  wire [1:0] index = i3 ? 2'd3 : i2 ? 2'd2 : i1 ? 2'd1 : 2'd0;
  assign {y1, y0} = index;
  assign y2 = i0 | i1 | i2 | i3;
  assign y3 = i0 ^ i1 ^ i2 ^ i3;
endmodule

module comb_majority(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  wire [2:0] count = {2'b0, i0} + {2'b0, i1} + {2'b0, i2} + {2'b0, i3};
  assign y0 = count >= 1;
  assign y1 = count == 2;
  assign y2 = count >= 3;
  assign y3 = count == 4;
endmodule

module comb_gray(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  wire [3:0] binary = {i3, i2, i1, i0};
  wire [3:0] gray = binary ^ (binary >> 1);
  assign {y3, y2, y1, y0} = gray;
endmodule

module comb_barrel(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  wire [1:0] data = {i3, i2};
  wire [1:0] amount = {i1, i0};
  wire [3:0] shifted = {2'b0, data} << amount;
  assign {y3, y2, y1, y0} = shifted;
endmodule

module comb_sbox(
    input i0, i1, i2, i3,
    output y0, y1, y2, y3
);
  reg [3:0] value;
  always @* begin
    case ({i3, i2, i1, i0})
      4'h0: value = 4'he;
      4'h1: value = 4'h4;
      4'h2: value = 4'hd;
      4'h3: value = 4'h1;
      4'h4: value = 4'h2;
      4'h5: value = 4'hf;
      4'h6: value = 4'hb;
      4'h7: value = 4'h8;
      4'h8: value = 4'h3;
      4'h9: value = 4'ha;
      4'ha: value = 4'h6;
      4'hb: value = 4'hc;
      4'hc: value = 4'h5;
      4'hd: value = 4'h9;
      4'he: value = 4'h0;
      default: value = 4'h7;
    endcase
  end
  assign {y3, y2, y1, y0} = value;
endmodule

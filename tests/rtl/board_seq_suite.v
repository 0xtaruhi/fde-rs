module seq_dff_bank(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [3:0] state;
  always @(posedge clk) begin
    if (rst) state <= 4'b0000;
    else state <= {i2, i1, i0, i0 ^ i1 ^ i2};
  end
  assign {y3, y2, y1, y0} = state;
endmodule

module seq_pipeline(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [3:0] pipe;
  wire serial = i0 ^ i1 ^ i2;
  always @(posedge clk) begin
    if (rst) pipe <= 4'b0000;
    else pipe <= {pipe[2:0], serial};
  end
  assign {y3, y2, y1, y0} = pipe;
endmodule

module seq_counter_up(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [3:0] count;
  wire [3:0] target = {1'b0, i2, i1, i0};
  always @(posedge clk) begin
    if (rst) count <= 4'b0000;
    else if (count < target) count <= count + 1'b1;
    else if (count > target) count <= count - 1'b1;
  end
  assign {y3, y2, y1, y0} = count;
endmodule

module seq_counter_down(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [3:0] count;
  wire [3:0] target = {1'b1, i2, i1, i0};
  always @(posedge clk) begin
    if (rst) count <= 4'b1111;
    else if (count > target) count <= count - 1'b1;
    else if (count < target) count <= count + 1'b1;
  end
  assign {y3, y2, y1, y0} = count;
endmodule

module seq_async_reset(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [3:0] state;
  always @(posedge clk or posedge rst) begin
    if (rst) state <= 4'b0000;
    else state <= {i0 & i2, i1 | i2, i0 ^ i1, ~i2};
  end
  assign {y3, y2, y1, y0} = state;
endmodule

module seq_enable(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [3:0] state;
  always @(posedge clk) begin
    if (rst) state <= 4'b0000;
    else if (i2) state <= {1'b1, i1, i0, i0 ^ i1};
  end
  assign {y3, y2, y1, y0} = state;
endmodule

module seq_feedback(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [3:0] state;
  always @(posedge clk) begin
    if (rst) begin
      state <= 4'b0000;
    end else begin
      state[0] <= state[0] | i0;
      state[1] <= state[1] | state[0] | i1;
      state[2] <= state[2] | state[1] | i2;
      state[3] <= state[3] | state[2];
    end
  end
  assign {y3, y2, y1, y0} = state;
endmodule

module seq_fsm(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [1:0] phase;
  reg [2:0] payload;
  always @(posedge clk) begin
    if (rst) begin
      phase <= 2'b00;
      payload <= 3'b000;
    end else if (phase != 2'b11) begin
      if (phase == 2'b00) payload <= {i2, i1, i0};
      phase <= phase + 1'b1;
    end
  end
  assign y0 = ^payload;
  assign y1 = phase[0];
  assign y2 = phase[1];
  assign y3 = phase == 2'b11;
endmodule

module seq_accumulator(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [3:0] accumulator;
  reg [2:0] cycles;
  wire [3:0] step = {1'b0, i2, i1, i0} + 1'b1;
  always @(posedge clk) begin
    if (rst) begin
      accumulator <= 4'b0000;
      cycles <= 3'b000;
    end else if (cycles != 3'd4) begin
      accumulator <= accumulator + step;
      cycles <= cycles + 1'b1;
    end
  end
  assign {y3, y2, y1, y0} = accumulator;
endmodule

module seq_onehot(
    input clk, rst, i0, i1, i2,
    output y0, y1, y2, y3
);
  reg [3:0] token;
  reg [3:0] payload;
  always @(posedge clk) begin
    if (rst) begin
      token <= 4'b0001;
      payload <= {1'b0, i2, i1, i0};
    end else if (!token[3]) begin
      token <= token << 1;
    end
  end
  assign {y3, y2, y1, y0} = token ^ payload;
endmodule

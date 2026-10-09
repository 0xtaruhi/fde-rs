module bram_selftest(input clk,rst_n, output reg done,fail,pass,
 output reg [7:0] first_addr, output reg [15:0] first_expect,first_read);
 localparam WA=0,RA_REQ=1,RA_WAIT=2,RA_CHECK=3,WB=4,RB_REQ=5,RB_WAIT=6,RB_CHECK=7,WP=8,RP_REQ=9,RP_WAIT=10,RP_CHECK=11,DONE=12;
 reg [3:0] state;
 reg [7:0] index;
 reg [15:0] errors;
 wire [7:0] addra=index;
 wire [7:0] addrb=(state==WP)?(index|8'b1):~index;
 function [15:0] pattern(input [7:0] a); begin pattern={a,~a}^16'h5a39; end endfunction
 wire wea=rst_n && ((state==WA)||(state==WP));
 wire web=rst_n && ((state==WB)||(state==WP));
 wire [15:0] dia=pattern(addra);
 wire [15:0] dib=(state==WB)?~pattern(addrb):pattern(addrb);
 wire [15:0] doa,dob;
 RAMB4_S16_S16 ram(.CLKA(clk),.CLKB(clk),.ADDRA(addra),.ADDRB(addrb),.DIA(dia),.DIB(dib),.DOA(doa),.DOB(dob),.WEA(wea),.WEB(web),.ENA(1'b1),.ENB(1'b1),.RSTA(1'b0),.RSTB(1'b0));
 wire invert=(state==RB_CHECK);
 wire [15:0] expect_a=pattern(addra)^{16{invert}};
 wire [15:0] expect_b=pattern(addrb)^{16{invert}};
 always @(posedge clk) begin
  if (!rst_n) begin
   state<=WA;index<=0;errors<=0;done<=0;fail<=0;pass<=0;first_addr<=0;first_expect<=0;first_read<=0;
  end else begin
   case(state)
    WA: if(index==255) begin index<=0;state<=RA_REQ;end else index<=index+1;
    RA_REQ: state<=RA_WAIT;
    RA_WAIT: state<=RA_CHECK;
    WB: if(index==255) begin index<=0;state<=RB_REQ;end else index<=index+1;
    RB_REQ: state<=RB_WAIT;
    RB_WAIT: state<=RB_CHECK;
    WP: if(index==254) begin index<=0;state<=RP_REQ;end else index<=index+2;
    RP_REQ: state<=RP_WAIT;
    RP_WAIT: state<=RP_CHECK;
    RA_CHECK,RB_CHECK,RP_CHECK: begin
     if(doa!=expect_a || dob!=expect_b) begin
      errors<=errors+1;
      if(errors==0) begin
       first_addr<=(doa!=expect_a)?addra:addrb;
       first_expect<=(doa!=expect_a)?expect_a:expect_b;
       first_read<=(doa!=expect_a)?doa:dob;
      end
     end
     if(index==255) begin
      index<=0;
      if(state==RA_CHECK) state<=WB;
      else if(state==RB_CHECK) state<=WP;
      else state<=DONE;
     end else begin
      index<=index+1;
      if(state==RA_CHECK) state<=RA_REQ;
      else if(state==RB_CHECK) state<=RB_REQ;
      else state<=RP_REQ;
     end
    end
    DONE: begin done<=1;pass<=(errors==0);fail<=(errors!=0);end
    default: state<=WA;
   endcase
  end
 end
endmodule

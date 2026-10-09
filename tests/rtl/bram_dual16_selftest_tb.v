module RAMB4_S16_S16(input CLKA,CLKB,input[7:0]ADDRA,ADDRB,input[15:0]DIA,DIB,input WEA,WEB,ENA,ENB,RSTA,RSTB,output reg[15:0]DOA,DOB);
reg[15:0]mem[0:255];
always @(posedge CLKA)begin if(WEA)mem[ADDRA]<=DIA;DOA<=mem[ADDRA];end
always @(posedge CLKB)begin if(WEB)mem[ADDRB]<=DIB;DOB<=mem[ADDRB];end
endmodule
module tb;
reg clk=0;always #5 clk=~clk;reg rst_n=0;wire done,pass,fail;wire[7:0]a;wire[15:0]e,r;
bram_selftest dut(clk,rst_n,done,fail,pass,a,e,r);
initial begin repeat(8)@(negedge clk);rst_n=1;repeat(5000)@(negedge clk);if(!done||!pass||fail)$fatal(1,"done=%b pass=%b fail=%b a=%x e=%x r=%x",done,pass,fail,a,e,r);$display("PASS full 256x16 dual-port autonomous test");$finish;end
endmodule

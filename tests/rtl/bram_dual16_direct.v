module bram_dual(input clk, input [7:0] addra,addrb, input [15:0] dia,dib, input wea,web, output [15:0] doa,dob, output reg heartbeat);
RAMB4_S16_S16 ram(.CLKA(clk),.CLKB(clk),.ADDRA(addra),.ADDRB(addrb),.DIA(dia),.DIB(dib),.DOA(doa),.DOB(dob),.WEA(wea),.WEB(web),.ENA(1'b1),.ENB(1'b1),.RSTA(1'b0),.RSTB(1'b0));
always @(posedge clk) heartbeat <= ~heartbeat;
endmodule

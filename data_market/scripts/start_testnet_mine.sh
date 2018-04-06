cd ..
remixd -s . &
ipfs daemon &
geth --rpc --rpcport "8545" --datadir pnet/ --port "30303" --ws --wsport "8546" --wsaddr "127.0.0.1" --rpcapi "personal,db,eth,net,web3" --wsapi "personal,db,eth,net,web3" --wsorigins "*" --unlock 0 --password <(echo "") --mine &
sleep 10
mist --rpc ./pnet/geth.ipc &
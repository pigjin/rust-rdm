// Local SSH authentication and forwarding fixture; never used by the application.
package main

import (
	"bytes"
	"flag"
	"fmt"
	"golang.org/x/crypto/ssh"
	"io"
	"net"
	"os"
	"strconv"
)

func main() {
	var addr, keyFile, authorized, redisAddr, socket string
	flag.StringVar(&addr, "addr", "127.0.0.1:0", "")
	flag.StringVar(&keyFile, "host-key", "", "")
	flag.StringVar(&authorized, "authorized-key", "", "")
	flag.StringVar(&redisAddr, "redis", "", "")
	flag.StringVar(&socket, "unix", "", "")
	flag.Parse()
	key, _ := os.ReadFile(keyFile)
	signer, err := ssh.ParsePrivateKey(key)
	if err != nil {
		panic(err)
	}
	pubData, _ := os.ReadFile(authorized)
	pub, _, _, _, err := ssh.ParseAuthorizedKey(pubData)
	if err != nil {
		panic(err)
	}
	config := &ssh.ServerConfig{PasswordCallback: func(c ssh.ConnMetadata, password []byte) (*ssh.Permissions, error) {
		if c.User() == "fixture-user" && string(password) == "rdm-password-fixture" {
			fmt.Fprintln(os.Stderr, "AUTH password")
			return nil, nil
		}
		return nil, fmt.Errorf("rejected fixture password")
	}, PublicKeyCallback: func(c ssh.ConnMetadata, key ssh.PublicKey) (*ssh.Permissions, error) {
		if c.User() == "fixture-user" && bytes.Equal(key.Marshal(), pub.Marshal()) {
			fmt.Fprintln(os.Stderr, "AUTH publickey")
			return nil, nil
		}
		return nil, fmt.Errorf("rejected fixture key")
	}}
	config.AddHostKey(signer)
	listener, err := net.Listen("tcp", addr)
	if err != nil {
		panic(err)
	}
	fmt.Println("SSH_FIXTURE_READY=" + listener.Addr().String())
	for {
		raw, err := listener.Accept()
		if err != nil {
			return
		}
		go func() {
			conn, channels, requests, err := ssh.NewServerConn(raw, config)
			if err != nil {
				raw.Close()
				return
			}
			defer conn.Close()
			go ssh.DiscardRequests(requests)
			for channel := range channels {
				network, target := "tcp", ""
				switch channel.ChannelType() {
				case "direct-tcpip":
					var payload struct {
						Host       string
						Port       uint32
						Origin     string
						OriginPort uint32
					}
					if err := ssh.Unmarshal(channel.ExtraData(), &payload); err == nil {
						target = net.JoinHostPort(payload.Host, strconv.Itoa(int(payload.Port)))
					}
					if target != redisAddr {
						channel.Reject(ssh.Prohibited, "fixture permits only its temporary Redis")
						continue
					}
				case "direct-streamlocal@openssh.com":
					var payload struct {
						Socket       string
						Reserved     string
						ReservedPort uint32
					}
					if err := ssh.Unmarshal(channel.ExtraData(), &payload); err == nil {
						target = payload.Socket
					}
					network = "unix"
					if socket == "" || target != socket {
						channel.Reject(ssh.Prohibited, "fixture permits only its temporary socket")
						continue
					}
				default:
					channel.Reject(ssh.UnknownChannelType, "fixture channel type")
					continue
				}
				remote, err := net.Dial(network, target)
				if err != nil {
					channel.Reject(ssh.ConnectionFailed, "fixture Redis unavailable")
					continue
				}
				ch, requests, err := channel.Accept()
				if err != nil {
					remote.Close()
					continue
				}
				go ssh.DiscardRequests(requests)
				fmt.Fprintln(os.Stderr, "FORWARD "+network)
				go func() {
					defer remote.Close()
					defer ch.Close()
					done := make(chan struct{})
					go func() {
						io.Copy(remote, ch)
						if tcp, ok := remote.(*net.TCPConn); ok {
							tcp.CloseWrite()
						}
						close(done)
					}()
					io.Copy(ch, remote)
					ch.CloseWrite()
					<-done
				}()
			}
		}()
	}
}

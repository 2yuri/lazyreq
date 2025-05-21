package main

import (
	"fmt"
	"log"
	"net/http"
)

func main() {
	http.HandleFunc("/api/v1/login", func(w http.ResponseWriter, r *http.Request) {
		fmt.Println("bateu no login")
		fmt.Println(r.Header.Get("Content-Type"))
		fmt.Println(r.Header.Get("Authorization"))

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, `{"token": "123"}`)
	})

	http.HandleFunc("/api/v1/me", func(w http.ResponseWriter, r *http.Request) {
		fmt.Println(r.Header.Get("Authorization"))
		fmt.Println(r.Header.Get("Content-Type"))

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, `{"id": "1234"}`)
	})

	http.HandleFunc("/api/v1/user/{id}", func(w http.ResponseWriter, r *http.Request) {
		fmt.Println(r.Header.Get("Authorization"))
		fmt.Println(r.Header.Get("Content-Type"))
		fmt.Println(r.PathValue("id"))

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, `{"barr": "122"}`)
	})

	log.Fatal(http.ListenAndServe(":8080", nil))

}
